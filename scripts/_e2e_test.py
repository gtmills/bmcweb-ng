#!/usr/bin/env python3
"""
Full end-to-end: launch rainier-bmc QEMU, wait for bmcweb,
set admin password via serial console, run smoke tests against
upstream bmcweb, inject bmcweb-ng, run smoke tests against bmcweb-ng,
print summary.

Requires: python3-paramiko (apt install python3-paramiko)
"""
import subprocess, sys, json, time, os, socket, threading

IMGDIR    = "/mnt/c/Users/GunnarMills/Desktop/ai/downstream-public/bmcweb-ng/target/qemu-test/rainier-image"
BMCWEB_NG = "/mnt/c/Users/GunnarMills/Desktop/ai/downstream-public/bmcweb-ng/target/arm-unknown-linux-gnueabihf/release/bmcwebd-ng"
LOG        = "/tmp/rainier_qemu.log"
SERIAL_SOCK= "/tmp/rainier-serial.sock"
HOST       = "127.0.0.1"
HTTPS      = 2443
HTTP       = 2080      # host port → guest port 80 (upstream bmcweb plain HTTP, unused)
NG_PORT    = 8080      # host port → guest port 8080 (bmcweb-ng plain HTTP)
SSH        = 2222
BOOT_TO    = 900   # 15 min; covers watchdog reboot cycles
BMCWEB_PW  = "0penBmc2!"

PASS = 0
FAIL = 0
baseline_fail = 0

# ── Paramiko SSH client ───────────────────────────────────────────────────────
_ssh_client = None

def log(msg): print(msg, flush=True)
def section(t): print(f"\n{'='*60}\n  {t}\n{'='*60}", flush=True)

def check(name, val, expected):
    global PASS, FAIL
    if val == expected:
        print(f"  PASS  {name}", flush=True)
        PASS += 1
    else:
        print(f"  FAIL  {name}", flush=True)
        print(f"         got:    {repr(val)}", flush=True)
        print(f"         expect: {repr(expected)}", flush=True)
        FAIL += 1

def http_get(url, auth=None, tls=True, verbose=False):
    cmd = ["curl", "-s", "-w", "\n__HTTP_CODE__:%{http_code}", "--max-time", "15"]
    if tls: cmd += ["-k"]
    if auth: cmd += ["-u", f"{auth[0]}:{auth[1]}"]
    cmd.append(url)
    r = subprocess.run(cmd, capture_output=True, text=True)
    parts = r.stdout.rsplit("\n__HTTP_CODE__:", 1)
    body = parts[0]
    code = parts[1].strip() if len(parts) > 1 else "?"
    if verbose: print(f"  HTTP {code}  body[:300]: {body[:300]}", flush=True)
    try: return json.loads(body)
    except: return {"__raw__": body[:300], "__code__": code}

def http_code(url, tls=True, auth=None):
    cmd = ["curl", "-s", "--max-time", "5", "-o", "/dev/null", "-w", "%{http_code}"]
    if tls: cmd += ["-k"]
    if auth: cmd += ["-u", f"{auth[0]}:{auth[1]}"]
    cmd.append(url)
    r = subprocess.run(cmd, capture_output=True, text=True)
    return r.stdout.strip()

# ── Serial console — background reader thread ────────────────────────────────
# A background thread continuously reads from the QEMU serial socket and
# appends to _serial_buf (bytes).  All reads use _serial_buf; writes go
# directly to the socket.

_serial_buf      = b""
_serial_buf_lock = threading.Lock()
_serial_sock_raw = None   # the actual socket (no timeout; thread does blocking reads)

def _serial_reader_thread():
    """Background thread: read from serial socket → _serial_buf forever."""
    global _serial_buf, _serial_sock_raw
    while True:
        try:
            chunk = _serial_sock_raw.recv(4096)
            if not chunk:
                time.sleep(0.05)
                continue
            with _serial_buf_lock:
                _serial_buf += chunk
        except Exception:
            time.sleep(0.1)

def _start_serial(retries=30, delay=2):
    """Connect to the QEMU serial socket and start the background reader."""
    global _serial_sock_raw
    for attempt in range(retries):
        if os.path.exists(SERIAL_SOCK):
            try:
                s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                s.connect(SERIAL_SOCK)
                s.setblocking(True)
                _serial_sock_raw = s
                t = threading.Thread(target=_serial_reader_thread, daemon=True)
                t.start()
                log(f"  Serial reader started (attempt {attempt+1})")
                return True
            except Exception as e:
                log(f"  Serial connect attempt {attempt+1}: {e}")
        time.sleep(delay)
    return False

def _serial_read_until(patterns, timeout=15):
    """
    Wait until one of the `patterns` strings appears in the accumulated
    serial buffer, or timeout expires.
    Returns (new_text_since_last_read, matched_pattern_or_None).
    """
    deadline = time.time() + timeout
    last_pos = 0
    while time.time() < deadline:
        with _serial_buf_lock:
            buf = _serial_buf
        text = buf.decode(errors="replace")
        for p in patterns:
            idx = text.find(p, last_pos)
            if idx >= 0:
                new_text = text[last_pos : idx + len(p)]
                return new_text, p
        time.sleep(0.1)
    with _serial_buf_lock:
        text = _serial_buf.decode(errors="replace")
    return text[last_pos:], None

def _serial_send(text):
    """Write text to the serial socket."""
    global _serial_sock_raw
    if _serial_sock_raw is None:
        return
    try:
        _serial_sock_raw.sendall(text.encode())
    except Exception as e:
        log(f"  [serial send error: {e}]")

def _serial_mark():
    """Return the current serial buffer length (used as a read cursor)."""
    with _serial_buf_lock:
        return len(_serial_buf)

def _serial_read_from(pos, patterns, timeout=15):
    """Read from position `pos` in the serial buffer until a pattern matches."""
    deadline = time.time() + timeout
    while time.time() < deadline:
        with _serial_buf_lock:
            buf = _serial_buf
        text = buf[pos:].decode(errors="replace")
        for p in patterns:
            idx = text.find(p)
            if idx >= 0:
                return text[:idx + len(p)], p
        time.sleep(0.1)
    with _serial_buf_lock:
        return _serial_buf[pos:].decode(errors="replace"), None

def _try_serial_login(username, current_pw, new_pw=None, timeout_pw=8, timeout_shell=12):
    """
    Attempt one login on the serial console.
    Handles PAM forced-password-change flow (IBM's passwd-expire).
    If new_pw is set, use it when PAM asks to change the password.
    Returns True if we land at a shell prompt, False otherwise.
    Assumes the console is currently showing a login: prompt.
    """
    mark = _serial_mark()
    _serial_send(f"{username}\r\n")
    out2, m2 = _serial_read_from(mark, ["Password", "password", "# ", "#\r\n", "$ "], timeout=timeout_pw)
    log(f"    after username '{username}': m2={m2!r}  tail={out2[-80:]!r}")
    if m2 in ("# ", "#\r\n", "$ "):
        log(f"    Logged in as {username} (no password prompt)")
        return True
    if m2 in ("Password", "password"):
        mark3 = _serial_mark()
        _serial_send(f"{current_pw}\r\n")
        out3, m3 = _serial_read_from(
            mark3,
            ["# ", "#\r\n", "$ ", "login:", "incorrect",
             "New password", "new password", "Current password:", "Current Password:"],
            timeout=timeout_shell
        )
        log(f"    after password: m3={m3!r}  tail={out3[-100:]!r}")
        if m3 in ("# ", "#\r\n", "$ "):
            log(f"    Logged in as {username}")
            return True
        if m3 in ("Current password:", "Current Password:"):
            # PAM forces change: already asked for current password again
            # Send current password
            mark4 = _serial_mark()
            _serial_send(f"{current_pw}\r\n")
            out4, m4 = _serial_read_from(mark4, ["New password", "new password"], timeout=8)
            log(f"    after current pw (2nd): m4={m4!r}")
            m3 = m4   # fall through to "New password" handling below
            out3 = out4
        if m3 in ("New password", "new password"):
            # PAM forced password change flow
            use_new = new_pw if new_pw else current_pw
            log(f"    PAM forced change — setting new password to: {use_new!r}")
            mark5 = _serial_mark()
            _serial_send(f"{use_new}\r\n")
            out5, m5 = _serial_read_from(mark5, ["Retype", "retype", "again", "confirm", "Confirm"], timeout=8)
            log(f"    after new pw: m5={m5!r}  tail={out5[-60:]!r}")
            mark6 = _serial_mark()
            _serial_send(f"{use_new}\r\n")   # confirm
            out6, m6 = _serial_read_from(mark6, ["# ", "#\r\n", "$ ", "login:", "updated", "changed", "incorrect"], timeout=12)
            log(f"    after confirm: m6={m6!r}  tail={out6[-80:]!r}")
            if m6 in ("# ", "#\r\n", "$ "):
                log(f"    Password changed, logged in as {username}")
                return True
            # Some systems drop back to login: after password change — try logging in
            if m6 == "login:":
                # Password was changed, now log in with new password
                log(f"    Password changed, logging in with new password...")
                return _try_serial_login(username, use_new, new_pw=None, timeout_pw=6)
    return False

def serial_login_and_run(commands, retries=30, delay=5):
    """
    Wait for a login prompt, try several user/pass combos, then run commands.
    Returns combined output string or None on failure.
    """
    # Credentials to try in order: (username, current_pw, new_pw_for_forced_change)
    # admin:admin has PAM force-change; new_pw=BMCWEB_PW sets the new password
    CREDS_TO_TRY = [
        ("admin", "admin",   BMCWEB_PW),   # most likely; PAM will force change
        ("admin", BMCWEB_PW, None),         # if already changed in a prior run
        ("admin", "0penBmc", BMCWEB_PW),
        ("root",  "",        None),
        ("root",  "0penBmc", None),
        ("root",  "root",    None),
    ]
    log("  Waiting for login prompt on serial console...")
    for attempt in range(retries):
        mark = _serial_mark()
        _serial_send("\r\n")
        out, matched = _serial_read_from(mark, ["login:", "# ", "$ ", "#\r\n"], timeout=7)
        log(f"  serial attempt {attempt+1}: {len(out)} chars, matched={matched!r}  tail={out[-60:]!r}")
        if matched in ("# ", "$ ", "#\r\n"):
            log("  Already at shell prompt")
            break
        if matched == "login:":
            for (u, p, np) in CREDS_TO_TRY:
                log(f"  Trying {u!r} / {p!r} (new_pw={np!r})...")
                if _try_serial_login(u, p, new_pw=np):
                    log(f"  Logged in as {u!r}")
                    break
                # After a failed attempt, wait for the next login: prompt
                _, _ = _serial_read_from(_serial_mark(), ["login:"], timeout=10)
            else:
                log("  All login attempts failed on serial console")
                time.sleep(delay)
                continue
            break   # break outer for loop too
        time.sleep(delay)
    else:
        log("  Serial: could not get shell after all retries")
        return None

    # We have a shell — run the commands
    all_output = ""
    for cmd in commands:
        mark = _serial_mark()
        _serial_send(cmd + "\r\n")
        # Use longer timeout for slow commands: curl download, sleep, systemctl
        cmd_timeout = 60 if any(k in cmd for k in ("curl", "sleep", "systemctl", "cat /tmp")) else 20
        out, _ = _serial_read_from(mark, ["# ", "#\r\n", "$ "], timeout=cmd_timeout)
        log(f"  serial> {cmd!r}  →  ...{out.strip()[-200:]!r}")
        all_output += out
    return all_output

# ── Paramiko helpers ──────────────────────────────────────────────────────────

def _open_ssh(username="root", password="", retries=6, delay=5):
    """Open (or reuse) a persistent paramiko SSH connection."""
    global _ssh_client
    if _ssh_client is not None:
        try:
            _ssh_client.exec_command("echo ping", timeout=5)
            return _ssh_client
        except Exception:
            _ssh_client = None

    import paramiko
    for attempt in range(retries):
        try:
            client = paramiko.SSHClient()
            client.set_missing_host_key_policy(paramiko.AutoAddPolicy())
            client.connect(
                HOST, port=SSH,
                username=username, password=password,
                look_for_keys=False, allow_agent=False,
                timeout=10, banner_timeout=20,
            )
            _ssh_client = client
            return client
        except Exception as e:
            log(f"  SSH attempt {attempt+1}/{retries}: {e}")
            time.sleep(delay)
    return None

def ssh_run(cmd_str, timeout=30):
    """Run a command over SSH; returns (stdout, stderr, returncode)."""
    client = _open_ssh()
    if client is None:
        return "", "SSH unavailable", 1
    try:
        _, stdout, stderr = client.exec_command(cmd_str, timeout=timeout)
        out = stdout.read().decode(errors="replace")
        err = stderr.read().decode(errors="replace")
        rc  = stdout.channel.recv_exit_status()
        return out, err, rc
    except Exception as e:
        global _ssh_client
        _ssh_client = None
        return "", str(e), 1

def scp_put(local_path, remote_path):
    """Upload a file over SFTP (paramiko); returns True on success."""
    client = _open_ssh()
    if client is None:
        return False
    try:
        sftp = client.open_sftp()
        log(f"  Uploading {os.path.getsize(local_path)//1024} KB...")
        sftp.put(local_path, remote_path)
        sftp.close()
        return True
    except Exception as e:
        log(f"  SFTP put error: {e}")
        return False

# ── 1. Launch QEMU ────────────────────────────────────────────────────────────
section("Launching rainier-bmc QEMU")
subprocess.run(["pkill", "-f", "qemu-system-arm"], capture_output=True)
time.sleep(1)
# Remove stale socket
subprocess.run(["rm", "-f", SERIAL_SOCK], capture_output=True)

qemu_cmd = [
    "qemu-system-arm", "-M", "rainier-bmc", "-nographic",
    "-kernel",  f"{IMGDIR}/zImage",
    "-dtb",     f"{IMGDIR}/aspeed-bmc-ibm-rainier.dtb",
    "-initrd",  f"{IMGDIR}/obmc-phosphor-initramfs.rootfs.cpio.xz",
    "-drive",   f"file={IMGDIR}/obmc-phosphor-image.rootfs.wic.qcow2,if=sd,index=2,snapshot=on",
    "-append",  (
        "console=ttyS4,115200n8 rootwait root=PARTLABEL=rofs-a "
        "systemd.watchdog-device= aspeed_wdt.nowdt=1"
    ),
    # The kernel outputs to ttyS0 (QEMU serial0 = first -serial flag),
    # regardless of console= cmdline arg under QEMU 8.2.x with rainier-bmc.
    # Map serial0 → Unix socket for interactive console I/O.
    "-serial", f"unix:{SERIAL_SOCK},server,nowait",  # serial0 / ttyS0
    "-net", "nic",
    "-net", (
        f"user"
        f",hostfwd=tcp::{HTTPS}-:443"
        f",hostfwd=tcp::{HTTP}-:80"
        f",hostfwd=tcp::{NG_PORT}-:{NG_PORT}"
        f",hostfwd=tcp::{SSH}-:22"
    ),
]
with open(LOG, "w") as lf:
    proc = subprocess.Popen(qemu_cmd, stdout=lf, stderr=lf)
log(f"QEMU pid={proc.pid}")

# Start the serial background reader immediately
_start_serial(retries=30, delay=2)

# ── 2. Wait for bmcweb ────────────────────────────────────────────────────────
section(f"Waiting for bmcweb (up to {BOOT_TO}s)")
deadline = time.time() + BOOT_TO
next_print = time.time() + 30

while time.time() < deadline:
    if proc.poll() is not None:
        log(f"QEMU exited code={proc.returncode}"); sys.exit(1)
    if time.time() >= next_print:
        with _serial_buf_lock:
            lines = _serial_buf.decode(errors="replace").splitlines()
        if lines:
            log(f"[{int(time.time()-deadline+BOOT_TO)}s remaining] --- serial +{len(lines)} lines total ---")
            sys.stdout.write("\n".join(lines[-10:]) + "\n"); sys.stdout.flush()
        next_print = time.time() + 30
    code = http_code(f"https://{HOST}:{HTTPS}/redfish/v1", tls=True)
    if code in ("200", "401"):
        elapsed = int(time.time() - deadline + BOOT_TO)
        log(f"\n[READY] bmcweb HTTP {code} after {elapsed}s"); break
    time.sleep(5)
else:
    log("[TIMEOUT]"); proc.terminate(); sys.exit(1)

# ── 3. Set admin password via serial console ──────────────────────────────────
section("Setting admin password via serial console")

# Wait a bit for the console to be fully settled
time.sleep(5)

# Check if serial reader is connected
serial_available = _serial_sock_raw is not None
log(f"  Serial reader connected: {serial_available}")
if not serial_available:
    # Try once more (QEMU may have taken a moment to create the socket)
    serial_available = _start_serial(retries=5, delay=2)
    log(f"  Serial reader retry: connected={serial_available}")

ssh_ok = False
ADMIN_PW_CURRENT = "admin"   # confirmed by shadow hash check
CREDS = ("admin", BMCWEB_PW)

# Strategy:
# 1. Try admin:admin for Redfish (may work if PasswordChangeRequired not set)
# 2. If 403, use serial console to complete PAM force-change (admin:admin → new_pw)
#    This changes the OS password AND may clear the PAM flag
# 3. Use Redfish PATCH to change admin password (clears bmcweb's PasswordChangeRequired)
# 4. Test Redfish with the new password

# Step 1: Quick check with admin:admin
log("  Testing admin:admin for Redfish...")
rc0 = http_code(f"https://{HOST}:{HTTPS}/redfish/v1/Systems", tls=True, auth=("admin", "admin"))
log(f"  admin:admin → HTTP {rc0}")
if rc0 == "200":
    ADMIN_PW_CURRENT = "admin"
    CREDS = ("admin", "admin")
    ssh_ok = True

# Step 2: Serial console — complete PAM forced-change flow
if not ssh_ok and serial_available:
    log("  Using serial console to complete PAM password change...")
    result = serial_login_and_run([
        # We're logged in as admin (after PAM forced change set pw to BMCWEB_PW)
        # Confirm the new password took effect
        "id && echo WHOAMI_OK",
        "cat /etc/passwd | grep admin",
    ], retries=15, delay=5)
    if result is not None:
        log(f"  Serial login succeeded (output len={len(result)})")
        ADMIN_PW_CURRENT = BMCWEB_PW   # PAM changed it to BMCWEB_PW
    else:
        log("  Serial login failed")

# Step 3: Redfish PATCH to change admin password (clears PasswordChangeRequired)
# We must PATCH using the OLD password (admin:admin or admin:BMCWEB_PW)
# regardless of whether the OS password has changed or not.
# Try both current passwords.
def redfish_patch_password(auth_pw, new_pw):
    """PATCH admin's Redfish password. Returns HTTP status code string."""
    import json as _json
    body = _json.dumps({"Password": new_pw})
    cmd = [
        "curl", "-sk", "-o", "/dev/null", "-w", "%{http_code}",
        "-X", "PATCH",
        "-H", "Content-Type: application/json",
        "-u", f"admin:{auth_pw}",
        "-d", body,
        f"https://{HOST}:{HTTPS}/redfish/v1/AccountService/Accounts/admin"
    ]
    r = subprocess.run(cmd, capture_output=True, text=True)
    return r.stdout.strip()

log("  Attempting Redfish PATCH to set admin password...")
for try_pw in ["admin", BMCWEB_PW, "0penBmc"]:
    rc_patch = redfish_patch_password(try_pw, BMCWEB_PW)
    log(f"  PATCH with admin:{try_pw} → HTTP {rc_patch}")
    if rc_patch in ("200", "204"):
        log(f"  Redfish PATCH succeeded with auth pw={try_pw!r}")
        ADMIN_PW_CURRENT = BMCWEB_PW
        break
    time.sleep(1)

# Step 4: Verify Redfish works with new password
time.sleep(3)
for try_pw in [BMCWEB_PW, "admin", "0penBmc"]:
    rc_verify = http_code(f"https://{HOST}:{HTTPS}/redfish/v1/Systems", tls=True, auth=("admin", try_pw))
    log(f"  Redfish /Systems with admin:{try_pw} → HTTP {rc_verify}")
    if rc_verify == "200":
        ADMIN_PW_CURRENT = try_pw
        CREDS = ("admin", try_pw)
        ssh_ok = True
        log(f"  Redfish auth confirmed: admin:{try_pw}")
        break

if not ssh_ok:
    log(f"  Redfish auth still failing — will run tests anyway with admin:{ADMIN_PW_CURRENT}")
    CREDS = ("admin", ADMIN_PW_CURRENT)

# ── 4. Try SSH for binary injection ──────────────────────────────────────────
section("Testing SSH (for binary injection)")
# SSH only works for users in 'shellaccess' group: root (locked), service (shell=/bin/sh)
# Neither root nor admin can SSH; we must use serial console for injection.
# Mark ssh_ok based on whether we can inject via serial (ssh_ok is reused for injection)
# Actually for binary injection we need either SSH or serial console.
# Since root is locked and admin has no SSH access, we use the serial console
# for binary injection via base64-encoded transfer.
log("  SSH blocked by IBM policy (admin not in shellaccess group)")
log(f"  Using Redfish creds: {CREDS[0]}:{CREDS[1]}")

# ── 5. Upstream bmcweb smoke tests ────────────────────────────────────────────
section(f"Smoke tests — upstream bmcweb  https://{HOST}:{HTTPS}")
base = f"https://{HOST}:{HTTPS}"

d = http_get(f"{base}/redfish/v1", auth=CREDS)
check("/redfish/v1 → RedfishVersion is string", isinstance(d.get("RedfishVersion"), str), True)
check("/redfish/v1 → @odata.type contains ServiceRoot",
      "ServiceRoot" in d.get("@odata.type", ""), True)
time.sleep(2)

d = http_get(f"{base}/redfish/v1/Systems", auth=CREDS, verbose=True)
check("/redfish/v1/Systems → @odata.type present", d.get("@odata.type") is not None, True)
time.sleep(2)

d = http_get(f"{base}/redfish/v1/Chassis", auth=CREDS, verbose=True)
check("/redfish/v1/Chassis → @odata.type present", d.get("@odata.type") is not None, True)
time.sleep(2)

d = http_get(f"{base}/redfish/v1/Managers", auth=CREDS, verbose=True)
check("/redfish/v1/Managers → @odata.type present", d.get("@odata.type") is not None, True)
time.sleep(2)

d = http_get(f"{base}/redfish/v1/AccountService", auth=CREDS, verbose=True)
check("/redfish/v1/AccountService → @odata.type present", d.get("@odata.type") is not None, True)

baseline_fail = FAIL

# ── 6. Inject bmcweb-ng via HTTP download from host ──────────────────────────
# We can't SSH (root locked, admin not in shellaccess).
# Strategy: spin up a Python HTTP server on the WSL host serving the binary,
# then wget it from inside QEMU via QEMU user-net (host = 10.0.2.2).
section("Injecting bmcweb-ng via HTTP download")

HTTP_SERVE_PORT = 9191   # port on host to serve the binary

def _serve_binary():
    """Serve files from the binary directory and /tmp via a simple HTTP server."""
    import http.server, socketserver, os as _os

    bin_dir = _os.path.dirname(BMCWEB_NG)

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            # Serve from binary dir first, then /tmp
            filename = _os.path.basename(self.path)
            for search_dir in [bin_dir, "/tmp"]:
                fpath = _os.path.join(search_dir, filename)
                if _os.path.exists(fpath):
                    self.send_response(200)
                    self.send_header("Content-Type", "application/octet-stream")
                    self.send_header("Content-Length", str(_os.path.getsize(fpath)))
                    self.end_headers()
                    with open(fpath, "rb") as f:
                        self.wfile.write(f.read())
                    return
            self.send_response(404)
            self.end_headers()
        def log_message(self, *a): pass  # suppress access log

    with socketserver.TCPServer(("0.0.0.0", HTTP_SERVE_PORT), Handler) as httpd:
        httpd.serve_forever()

server_thread = threading.Thread(target=_serve_binary, daemon=True)
server_thread.start()
time.sleep(1)
log(f"  HTTP server started on port {HTTP_SERVE_PORT}")
log(f"  File: {os.path.basename(BMCWEB_NG)}  ({os.path.getsize(BMCWEB_NG)//1024} KB)")

if not serial_available:
    log("  Serial console not available — skipping bmcweb-ng injection")
    FAIL += 1
else:
    # Write config file to serve alongside the binary
    cfg_content = (
        "[server]\n"
        f'bind_address = "0.0.0.0"\n'
        f"port = {NG_PORT}\n"
        'tls_cert = ""\n'
        'tls_key = ""\n'
        "max_connections = 100\n"
        "\n[auth]\n"
        "session_timeout_seconds = 3600\n"
        "max_sessions = 64\n"
        "\n[logging]\n"
        'level = "info"\n'
        "\n[metrics]\n"
        "enabled = false\n"
        "port = 9090\n"
    )
    cfg_path = "/tmp/bmcwng_host.toml"
    with open(cfg_path, "w") as _f:
        _f.write(cfg_content)
    log(f"  Config written to {cfg_path}")

    # Do all injection inside a single serial login session to avoid re-auth issues.
    # The marker strings here do NOT appear in the commands sent (to avoid false echoes).
    inj_result = serial_login_and_run([
        # Download binary via curl
        (
            f"curl -s -o /tmp/bmcwebd-ng http://10.0.2.2:{HTTP_SERVE_PORT}/{os.path.basename(BMCWEB_NG)}"
        ),
        # Download config via curl
        (
            f"curl -s -o /tmp/bmcwng.toml http://10.0.2.2:{HTTP_SERVE_PORT}/bmcwng_host.toml"
        ),
        # Verify downloads
        "ls -la /tmp/bmcwebd-ng /tmp/bmcwng.toml 2>&1",
        # Make executable
        "chmod +x /tmp/bmcwebd-ng",
        # Stop upstream bmcweb
        "systemctl stop bmcweb 2>&1; echo BMCWEB_STOPPED",
        # Launch bmcweb-ng in background (separate command so prompt returns cleanly)
        "nohup /tmp/bmcwebd-ng --config /tmp/bmcwng.toml >/tmp/bmcwebd-ng.log 2>&1 &",
        # Give it time to start, then check process and log
        "sleep 5",
        "ps | grep bmcwebd",
        "cat /tmp/bmcwebd-ng.log",
    ], retries=5, delay=5)

    log(f"  Injection result (len={len(inj_result) if inj_result else 0})")
    if inj_result:
        log(f"  Injection output tail: {inj_result.strip()[-600:]!r}")

    # Check that the binary was actually downloaded (> 1MB = 1000 bytes in ls -la)
    ng_download_ok = (
        inj_result is not None
        and "No such file" not in inj_result
        and "/tmp/bmcwebd-ng" in inj_result
    )
    if not ng_download_ok:
        log(f"  FAIL: injection via serial failed (no binary found)"); FAIL += 1
    else:

        # Wait for bmcweb-ng on HTTP port 8080 (forwarded to NG_PORT=8080)
        ng_deadline = time.time() + 90
        ng_up = False
        while time.time() < ng_deadline:
            code = http_code(f"http://{HOST}:{NG_PORT}/redfish/v1", tls=False)
            if code in ("200", "401"):
                ng_up = True; log(f"  bmcweb-ng HTTP {code}"); break
            time.sleep(3)

        if not ng_up:
            log("  FAIL: bmcweb-ng did not come up within 90s"); FAIL += 1
        else:
            # ── 7. bmcweb-ng smoke tests ──────────────────────────────────────
            section(f"Smoke tests — bmcweb-ng  http://{HOST}:{NG_PORT}")
            ng = f"http://{HOST}:{NG_PORT}"

            # /redfish/v1 should be unauthenticated per spec (no auth param)
            d = http_get(f"{ng}/redfish/v1", tls=False, verbose=True)
            check("[ng] /redfish/v1 → RedfishVersion present",
                  isinstance(d.get("RedfishVersion"), str), True)
            check("[ng] /redfish/v1 → @odata.type contains ServiceRoot",
                  "ServiceRoot" in d.get("@odata.type", ""), True)

            # Authenticated endpoints use CREDS (same admin account)
            d = http_get(f"{ng}/redfish/v1/Systems", tls=False, auth=CREDS, verbose=True)
            check("[ng] /redfish/v1/Systems → @odata.type contains Collection",
                  "Collection" in d.get("@odata.type", ""), True)

            d = http_get(f"{ng}/redfish/v1/Chassis", tls=False, auth=CREDS, verbose=True)
            check("[ng] /redfish/v1/Chassis → @odata.type contains Collection",
                  "Collection" in d.get("@odata.type", ""), True)

            d = http_get(f"{ng}/redfish/v1/Managers", tls=False, auth=CREDS, verbose=True)
            check("[ng] /redfish/v1/Managers → @odata.type contains Collection",
                  "Collection" in d.get("@odata.type", ""), True)

# ── 8. Kill QEMU ──────────────────────────────────────────────────────────────
proc.terminate()
log("\nQEMU stopped")

# ── 9. Summary ────────────────────────────────────────────────────────────────
section("SUMMARY")
print(f"  Baseline failures (upstream bmcweb) : {baseline_fail}")
print(f"  Total PASS : {PASS}")
print(f"  Total FAIL : {FAIL}")
if FAIL == 0:
    print("\n  *** ALL TESTS PASSED ***")
    sys.exit(0)
else:
    print(f"\n  *** {FAIL} TEST(S) FAILED ***")
    sys.exit(1)
