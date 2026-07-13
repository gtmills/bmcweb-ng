#!/usr/bin/env python3
"""
Full end-to-end: launch rainier-bmc QEMU, wait for bmcweb,
set admin password via serial console, run smoke tests against
upstream bmcweb, inject bmcweb-ng, run smoke tests against bmcweb-ng,
print summary.

Paths are derived from this script's location — no hardcoded user paths.
The repo root is two directories above scripts/, i.e. scripts/../..

Requires: python3-paramiko (apt install python3-paramiko)
"""
import subprocess, sys, json, time, os, socket, threading

# ── Path resolution — no hardcoded user paths ─────────────────────────────────
# This script lives at <repo>/scripts/_e2e_test.py
# Repo root is therefore scripts/../../  (i.e. the bmcweb-ng checkout dir)
_SCRIPT_DIR = os.path.dirname(os.path.realpath(__file__))
_REPO_ROOT  = os.path.normpath(os.path.join(_SCRIPT_DIR, ".."))

IMGDIR    = os.path.join(_REPO_ROOT, "target", "qemu-test", "rainier-image")
BMCWEB_NG = os.path.join(_REPO_ROOT, "target", "arm-unknown-linux-gnueabihf",
                         "release", "bmcwebd-ng")
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

def check_in(name, val, choices):
    """Pass if val is one of the given choices."""
    global PASS, FAIL
    if val in choices:
        print(f"  PASS  {name}", flush=True)
        PASS += 1
    else:
        print(f"  FAIL  {name}", flush=True)
        print(f"         got:    {repr(val)}", flush=True)
        print(f"         expect one of: {choices}", flush=True)
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
    try:
        d = json.loads(body)
        d["__code__"] = code
        return d
    except:
        return {"__raw__": body[:300], "__code__": code}

def http_post(url, body_dict, auth=None, tls=True):
    """POST JSON body; returns (parsed_dict_or_raw, http_code_str)."""
    body = json.dumps(body_dict)
    cmd = ["curl", "-s", "-w", "\n__HTTP_CODE__:%{http_code}", "--max-time", "15",
           "-X", "POST", "-H", "Content-Type: application/json", "-d", body]
    if tls: cmd += ["-k"]
    if auth: cmd += ["-u", f"{auth[0]}:{auth[1]}"]
    cmd.append(url)
    r = subprocess.run(cmd, capture_output=True, text=True)
    parts = r.stdout.rsplit("\n__HTTP_CODE__:", 1)
    body_out = parts[0]
    code = parts[1].strip() if len(parts) > 1 else "?"
    try:
        return json.loads(body_out), code
    except:
        return {"__raw__": body_out[:300]}, code

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
            mark4 = _serial_mark()
            _serial_send(f"{current_pw}\r\n")
            out4, m4 = _serial_read_from(mark4, ["New password", "new password"], timeout=8)
            log(f"    after current pw (2nd): m4={m4!r}")
            m3 = m4
            out3 = out4
        if m3 in ("New password", "new password"):
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
            if m6 == "login:":
                log(f"    Password changed, logging in with new password...")
                return _try_serial_login(username, use_new, new_pw=None, timeout_pw=6)
    return False

def serial_login_and_run(commands, retries=30, delay=5):
    """
    Wait for a login prompt, try several user/pass combos, then run commands.
    Returns combined output string or None on failure.
    """
    CREDS_TO_TRY = [
        ("admin", "admin",   BMCWEB_PW),
        ("admin", BMCWEB_PW, None),
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
                _, _ = _serial_read_from(_serial_mark(), ["login:"], timeout=10)
            else:
                log("  All login attempts failed on serial console")
                time.sleep(delay)
                continue
            break
        time.sleep(delay)
    else:
        log("  Serial: could not get shell after all retries")
        return None

    all_output = ""
    for cmd in commands:
        mark = _serial_mark()
        _serial_send(cmd + "\r\n")
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

# ── 1. Launch QEMU ────────────────────────────────────────────────────────────
section("Launching rainier-bmc QEMU")
log(f"  Repo root : {_REPO_ROOT}")
log(f"  Image dir : {IMGDIR}")
log(f"  Binary    : {BMCWEB_NG}")

if not os.path.isdir(IMGDIR):
    log(f"  ERROR: image dir not found: {IMGDIR}")
    sys.exit(1)
if not os.path.isfile(BMCWEB_NG):
    log(f"  ERROR: bmcwebd-ng binary not found: {BMCWEB_NG}")
    log("  Run: cargo build --release --target arm-unknown-linux-gnueabihf")
    sys.exit(1)

subprocess.run(["pkill", "-f", "qemu-system-arm"], capture_output=True)
time.sleep(1)
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
    "-serial", f"unix:{SERIAL_SOCK},server,nowait",
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

time.sleep(5)
serial_available = _serial_sock_raw is not None
log(f"  Serial reader connected: {serial_available}")
if not serial_available:
    serial_available = _start_serial(retries=5, delay=2)
    log(f"  Serial reader retry: connected={serial_available}")

ssh_ok = False
ADMIN_PW_CURRENT = "admin"
CREDS = ("admin", BMCWEB_PW)

log("  Testing admin:admin for Redfish...")
rc0 = http_code(f"https://{HOST}:{HTTPS}/redfish/v1/Systems", tls=True, auth=("admin", "admin"))
log(f"  admin:admin → HTTP {rc0}")
if rc0 == "200":
    ADMIN_PW_CURRENT = "admin"
    CREDS = ("admin", "admin")
    ssh_ok = True

if not ssh_ok and serial_available:
    log("  Using serial console to complete PAM password change...")
    result = serial_login_and_run([
        "id && echo WHOAMI_OK",
        "cat /etc/passwd | grep admin",
    ], retries=15, delay=5)
    if result is not None:
        log(f"  Serial login succeeded (output len={len(result)})")
        ADMIN_PW_CURRENT = BMCWEB_PW
    else:
        log("  Serial login failed")

def redfish_patch_password(auth_pw, new_pw):
    """PATCH admin's Redfish password. Returns HTTP status code string."""
    body = json.dumps({"Password": new_pw})
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

# ── 4. SSH note ───────────────────────────────────────────────────────────────
section("Testing SSH (for binary injection)")
log("  SSH blocked by IBM policy (admin not in shellaccess group)")
log(f"  Using Redfish creds: {CREDS[0]}:{CREDS[1]}")

# ── 5. Upstream bmcweb smoke tests ────────────────────────────────────────────
section(f"Smoke tests — upstream bmcweb  https://{HOST}:{HTTPS}")
base = f"https://{HOST}:{HTTPS}"

# ServiceRoot
d = http_get(f"{base}/redfish/v1", auth=CREDS)
check("/redfish/v1 → RedfishVersion is string", isinstance(d.get("RedfishVersion"), str), True)
check("/redfish/v1 → @odata.type contains ServiceRoot",
      "ServiceRoot" in d.get("@odata.type", ""), True)
time.sleep(1)

# Systems collection
d = http_get(f"{base}/redfish/v1/Systems", auth=CREDS, verbose=True)
check("/redfish/v1/Systems → @odata.type present", d.get("@odata.type") is not None, True)
check("/redfish/v1/Systems → Members is list", isinstance(d.get("Members"), list), True)
time.sleep(1)

# System instance
d = http_get(f"{base}/redfish/v1/Systems/system", auth=CREDS)
check("/redfish/v1/Systems/system → @odata.type present", d.get("@odata.type") is not None, True)
check("/redfish/v1/Systems/system → Id present", d.get("Id") is not None, True)
time.sleep(1)

# Chassis
d = http_get(f"{base}/redfish/v1/Chassis", auth=CREDS, verbose=True)
check("/redfish/v1/Chassis → @odata.type present", d.get("@odata.type") is not None, True)
time.sleep(1)

# Managers
d = http_get(f"{base}/redfish/v1/Managers", auth=CREDS, verbose=True)
check("/redfish/v1/Managers → @odata.type present", d.get("@odata.type") is not None, True)
time.sleep(1)

# AccountService
d = http_get(f"{base}/redfish/v1/AccountService", auth=CREDS, verbose=True)
check("/redfish/v1/AccountService → @odata.type present", d.get("@odata.type") is not None, True)
time.sleep(1)

# SessionService
d = http_get(f"{base}/redfish/v1/SessionService", auth=CREDS)
check("/redfish/v1/SessionService → @odata.type present", d.get("@odata.type") is not None, True)
time.sleep(1)

baseline_fail = FAIL

# ── 6. Inject bmcweb-ng via HTTP download from host ──────────────────────────
section("Injecting bmcweb-ng via HTTP download")

HTTP_SERVE_PORT = 9191

def _serve_binary():
    """Serve files from the binary directory and /tmp via a simple HTTP server."""
    import http.server, socketserver, os as _os

    bin_dir = _os.path.dirname(BMCWEB_NG)

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
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
        def log_message(self, *a): pass

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

    inj_result = serial_login_and_run([
        f"curl -s -o /tmp/bmcwebd-ng http://10.0.2.2:{HTTP_SERVE_PORT}/{os.path.basename(BMCWEB_NG)}",
        f"curl -s -o /tmp/bmcwng.toml http://10.0.2.2:{HTTP_SERVE_PORT}/bmcwng_host.toml",
        "ls -la /tmp/bmcwebd-ng /tmp/bmcwng.toml 2>&1",
        "chmod +x /tmp/bmcwebd-ng",
        "systemctl stop bmcweb 2>&1; echo BMCWEB_STOPPED",
        "nohup /tmp/bmcwebd-ng --config /tmp/bmcwng.toml >/tmp/bmcwebd-ng.log 2>&1 &",
        "sleep 5",
        "ps | grep bmcwebd",
        "cat /tmp/bmcwebd-ng.log",
    ], retries=5, delay=5)

    log(f"  Injection result (len={len(inj_result) if inj_result else 0})")
    if inj_result:
        log(f"  Injection output tail: {inj_result.strip()[-600:]!r}")

    ng_download_ok = (
        inj_result is not None
        and "No such file" not in inj_result
        and "/tmp/bmcwebd-ng" in inj_result
    )
    if not ng_download_ok:
        log(f"  FAIL: injection via serial failed (no binary found)"); FAIL += 1
    else:
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
            # Tests cover every collection and major instance endpoint exposed
            # by the router in src/api/redfish/mod.rs, plus session creation,
            # account listing, and negative-path (404) checks.
            section(f"Smoke tests — bmcweb-ng  http://{HOST}:{NG_PORT}")
            ng = f"http://{HOST}:{NG_PORT}"

            # ── ServiceRoot (unauthenticated per Redfish spec §7.3.1) ─────────
            d = http_get(f"{ng}/redfish/v1", tls=False, verbose=True)
            check("[ng] /redfish/v1 → RedfishVersion present",
                  isinstance(d.get("RedfishVersion"), str), True)
            check("[ng] /redfish/v1 → @odata.type contains ServiceRoot",
                  "ServiceRoot" in d.get("@odata.type", ""), True)
            check("[ng] /redfish/v1 → Systems link present",
                  d.get("Systems", {}).get("@odata.id") == "/redfish/v1/Systems", True)
            check("[ng] /redfish/v1 → Chassis link present",
                  d.get("Chassis", {}).get("@odata.id") == "/redfish/v1/Chassis", True)
            check("[ng] /redfish/v1 → Managers link present",
                  d.get("Managers", {}).get("@odata.id") == "/redfish/v1/Managers", True)
            check("[ng] /redfish/v1 → @odata.id is /redfish/v1",
                  d.get("@odata.id") == "/redfish/v1", True)
            check("[ng] /redfish/v1 → HTTP 200 unauthenticated",
                  d.get("__code__") == "200", True)
            time.sleep(1)

            # ── Systems ──────────────────────────────────────────────────────
            d = http_get(f"{ng}/redfish/v1/Systems", tls=False, auth=CREDS, verbose=True)
            check("[ng] /redfish/v1/Systems → @odata.type contains Collection",
                  "Collection" in d.get("@odata.type", ""), True)
            check("[ng] /redfish/v1/Systems → Members is list",
                  isinstance(d.get("Members"), list), True)
            check("[ng] /redfish/v1/Systems → Members@odata.count >= 1",
                  isinstance(d.get("Members@odata.count"), int) and d.get("Members@odata.count", 0) >= 1, True)
            time.sleep(1)

            d = http_get(f"{ng}/redfish/v1/Systems/system", tls=False, auth=CREDS)
            check("[ng] /redfish/v1/Systems/system → @odata.type present",
                  d.get("@odata.type") is not None, True)
            check("[ng] /redfish/v1/Systems/system → Id == 'system'",
                  d.get("Id") == "system", True)
            check("[ng] /redfish/v1/Systems/system → Name present",
                  isinstance(d.get("Name"), str), True)
            check("[ng] /redfish/v1/Systems/system → @odata.id correct",
                  d.get("@odata.id") == "/redfish/v1/Systems/system", True)
            time.sleep(1)

            d = http_get(f"{ng}/redfish/v1/Systems/system/Processors", tls=False, auth=CREDS)
            check("[ng] /Systems/system/Processors → @odata.type present",
                  d.get("@odata.type") is not None, True)
            time.sleep(1)

            d = http_get(f"{ng}/redfish/v1/Systems/system/Memory", tls=False, auth=CREDS)
            check("[ng] /Systems/system/Memory → @odata.type present",
                  d.get("@odata.type") is not None, True)
            time.sleep(1)

            d = http_get(f"{ng}/redfish/v1/Systems/system/Storage", tls=False, auth=CREDS)
            check("[ng] /Systems/system/Storage → @odata.type present",
                  d.get("@odata.type") is not None, True)
            time.sleep(1)

            d = http_get(f"{ng}/redfish/v1/Systems/system/EthernetInterfaces", tls=False, auth=CREDS)
            check("[ng] /Systems/system/EthernetInterfaces → @odata.type present",
                  d.get("@odata.type") is not None, True)
            time.sleep(1)

            d = http_get(f"{ng}/redfish/v1/Systems/system/LogServices", tls=False, auth=CREDS)
            check("[ng] /Systems/system/LogServices → @odata.type present",
                  d.get("@odata.type") is not None, True)
            time.sleep(1)

            d = http_get(f"{ng}/redfish/v1/Systems/system/LogServices/EventLog", tls=False, auth=CREDS)
            check("[ng] /Systems/system/LogServices/EventLog → @odata.type present",
                  d.get("@odata.type") is not None, True)
            time.sleep(1)

            d = http_get(f"{ng}/redfish/v1/Systems/system/LogServices/EventLog/Entries", tls=False, auth=CREDS, verbose=True)
            check("[ng] /Systems/system/LogServices/EventLog/Entries → HTTP 200",
                  d.get("__code__") == "200", True)
            check("[ng] /Systems/system/LogServices/EventLog/Entries → @odata.type present",
                  d.get("@odata.type") is not None, True)
            time.sleep(1)

            # 404 for a non-existent system
            c = http_code(f"{ng}/redfish/v1/Systems/nonexistent", tls=False, auth=CREDS)
            check("[ng] /Systems/nonexistent → HTTP 404",
                  c == "404", True)
            time.sleep(1)

            # ── Chassis ──────────────────────────────────────────────────────
            d = http_get(f"{ng}/redfish/v1/Chassis", tls=False, auth=CREDS, verbose=True)
            check("[ng] /redfish/v1/Chassis → @odata.type contains Collection",
                  "Collection" in d.get("@odata.type", ""), True)
            time.sleep(1)

            d = http_get(f"{ng}/redfish/v1/Chassis/chassis", tls=False, auth=CREDS)
            check("[ng] /Chassis/chassis → @odata.type present",
                  d.get("@odata.type") is not None, True)
            check("[ng] /Chassis/chassis → Id == 'chassis'",
                  d.get("Id") == "chassis", True)
            time.sleep(1)

            d = http_get(f"{ng}/redfish/v1/Chassis/chassis/Power", tls=False, auth=CREDS)
            check("[ng] /Chassis/chassis/Power → @odata.type present",
                  d.get("@odata.type") is not None, True)
            time.sleep(1)

            d = http_get(f"{ng}/redfish/v1/Chassis/chassis/Thermal", tls=False, auth=CREDS)
            check("[ng] /Chassis/chassis/Thermal → @odata.type present",
                  d.get("@odata.type") is not None, True)
            time.sleep(1)

            d = http_get(f"{ng}/redfish/v1/Chassis/chassis/Sensors", tls=False, auth=CREDS)
            check("[ng] /Chassis/chassis/Sensors → @odata.type present",
                  d.get("@odata.type") is not None, True)
            time.sleep(1)

            # ── Managers ─────────────────────────────────────────────────────
            d = http_get(f"{ng}/redfish/v1/Managers", tls=False, auth=CREDS, verbose=True)
            check("[ng] /redfish/v1/Managers → @odata.type contains Collection",
                  "Collection" in d.get("@odata.type", ""), True)
            time.sleep(1)

            d = http_get(f"{ng}/redfish/v1/Managers/bmc", tls=False, auth=CREDS)
            check("[ng] /Managers/bmc → @odata.type present",
                  d.get("@odata.type") is not None, True)
            check("[ng] /Managers/bmc → Id == 'bmc'",
                  d.get("Id") == "bmc", True)
            time.sleep(1)

            d = http_get(f"{ng}/redfish/v1/Managers/bmc/NetworkProtocol", tls=False, auth=CREDS)
            check("[ng] /Managers/bmc/NetworkProtocol → @odata.type present",
                  d.get("@odata.type") is not None, True)
            time.sleep(1)

            d = http_get(f"{ng}/redfish/v1/Managers/bmc/EthernetInterfaces", tls=False, auth=CREDS)
            check("[ng] /Managers/bmc/EthernetInterfaces → @odata.type present",
                  d.get("@odata.type") is not None, True)
            time.sleep(1)

            d = http_get(f"{ng}/redfish/v1/Managers/bmc/LogServices", tls=False, auth=CREDS)
            check("[ng] /Managers/bmc/LogServices → @odata.type present",
                  d.get("@odata.type") is not None, True)
            time.sleep(1)

            d = http_get(f"{ng}/redfish/v1/Managers/bmc/LogServices/BMC", tls=False, auth=CREDS)
            check("[ng] /Managers/bmc/LogServices/BMC → @odata.type present",
                  d.get("@odata.type") is not None, True)
            time.sleep(1)

            d = http_get(f"{ng}/redfish/v1/Managers/bmc/LogServices/BMC/Entries", tls=False, auth=CREDS)
            check("[ng] /Managers/bmc/LogServices/BMC/Entries → @odata.type present",
                  d.get("@odata.type") is not None, True)
            time.sleep(1)

            # ── SessionService ────────────────────────────────────────────────
            d = http_get(f"{ng}/redfish/v1/SessionService", tls=False, auth=CREDS)
            check("[ng] /SessionService → @odata.type present",
                  d.get("@odata.type") is not None, True)
            time.sleep(1)

            d = http_get(f"{ng}/redfish/v1/SessionService/Sessions", tls=False, auth=CREDS)
            check("[ng] /SessionService/Sessions → @odata.type present",
                  d.get("@odata.type") is not None, True)
            check("[ng] /SessionService/Sessions → Members is list",
                  isinstance(d.get("Members"), list), True)
            time.sleep(1)

            # Session creation (POST — unauthenticated login endpoint)
            sess_body, sess_code = http_post(
                f"{ng}/redfish/v1/SessionService/Sessions",
                {"UserName": CREDS[0], "Password": CREDS[1]},
                tls=False
            )
            check("[ng] POST /SessionService/Sessions → HTTP 201 or 200",
                  sess_code in ("200", "201"), True)
            time.sleep(1)

            # ── AccountService ────────────────────────────────────────────────
            d = http_get(f"{ng}/redfish/v1/AccountService", tls=False, auth=CREDS, verbose=True)
            check("[ng] /AccountService → @odata.type present",
                  d.get("@odata.type") is not None, True)
            time.sleep(1)

            d = http_get(f"{ng}/redfish/v1/AccountService/Accounts", tls=False, auth=CREDS)
            check("[ng] /AccountService/Accounts → @odata.type present",
                  d.get("@odata.type") is not None, True)
            check("[ng] /AccountService/Accounts → Members is list",
                  isinstance(d.get("Members"), list), True)
            time.sleep(1)

            d = http_get(f"{ng}/redfish/v1/AccountService/Roles", tls=False, auth=CREDS)
            check("[ng] /AccountService/Roles → @odata.type present",
                  d.get("@odata.type") is not None, True)
            check("[ng] /AccountService/Roles → Members is list",
                  isinstance(d.get("Members"), list), True)
            time.sleep(1)

            # Built-in Administrator role
            d = http_get(f"{ng}/redfish/v1/AccountService/Roles/Administrator", tls=False, auth=CREDS)
            check("[ng] /AccountService/Roles/Administrator → Id == 'Administrator'",
                  d.get("Id") == "Administrator", True)
            time.sleep(1)

            # ── Registries / JsonSchemas ──────────────────────────────────────
            d = http_get(f"{ng}/redfish/v1/Registries", tls=False, auth=CREDS)
            check("[ng] /Registries → @odata.type present",
                  d.get("@odata.type") is not None, True)
            time.sleep(1)

            d = http_get(f"{ng}/redfish/v1/JsonSchemas", tls=False, auth=CREDS)
            check("[ng] /JsonSchemas → @odata.type present",
                  d.get("@odata.type") is not None, True)
            time.sleep(1)

            # ── EventService ──────────────────────────────────────────────────
            d = http_get(f"{ng}/redfish/v1/EventService", tls=False, auth=CREDS)
            check("[ng] /EventService → @odata.type present",
                  d.get("@odata.type") is not None, True)
            time.sleep(1)

            d = http_get(f"{ng}/redfish/v1/EventService/Subscriptions", tls=False, auth=CREDS)
            check("[ng] /EventService/Subscriptions → @odata.type present",
                  d.get("@odata.type") is not None, True)
            time.sleep(1)

            # ── TaskService ───────────────────────────────────────────────────
            d = http_get(f"{ng}/redfish/v1/TaskService", tls=False, auth=CREDS)
            check("[ng] /TaskService → @odata.type present",
                  d.get("@odata.type") is not None, True)
            time.sleep(1)

            d = http_get(f"{ng}/redfish/v1/TaskService/Tasks", tls=False, auth=CREDS)
            check("[ng] /TaskService/Tasks → @odata.type present",
                  d.get("@odata.type") is not None, True)
            time.sleep(1)

            # ── UpdateService ─────────────────────────────────────────────────
            d = http_get(f"{ng}/redfish/v1/UpdateService", tls=False, auth=CREDS)
            check("[ng] /UpdateService → @odata.type present",
                  d.get("@odata.type") is not None, True)
            time.sleep(1)

            d = http_get(f"{ng}/redfish/v1/UpdateService/FirmwareInventory", tls=False, auth=CREDS)
            check("[ng] /UpdateService/FirmwareInventory → @odata.type present",
                  d.get("@odata.type") is not None, True)
            time.sleep(1)

            # ── CertificateService ────────────────────────────────────────────
            d = http_get(f"{ng}/redfish/v1/CertificateService", tls=False, auth=CREDS)
            check("[ng] /CertificateService → @odata.type present",
                  d.get("@odata.type") is not None, True)
            time.sleep(1)

            # ── TelemetryService ──────────────────────────────────────────────
            d = http_get(f"{ng}/redfish/v1/TelemetryService", tls=False, auth=CREDS)
            check("[ng] /TelemetryService → @odata.type present",
                  d.get("@odata.type") is not None, True)
            time.sleep(1)

            # ── Health endpoint (non-Redfish) ─────────────────────────────────
            c = http_code(f"{ng}/health", tls=False)
            check("[ng] /health → HTTP 200", c == "200", True)
            time.sleep(1)

            # ── Negative: require auth on protected endpoint ───────────────────
            c = http_code(f"{ng}/redfish/v1/Systems", tls=False)
            check("[ng] /Systems without auth → HTTP 401",
                  c == "401", True)

# ── 8. Kill QEMU ──────────────────────────────────────────────────────────────
# When SKIP_TEARDOWN=1 (set by _run_validator.sh) keep QEMU alive so the
# validator can run against bmcweb-ng, then wait until killed externally.
# In this mode the Summary section is also skipped so sys.exit() is not called.
if os.environ.get("SKIP_TEARDOWN") == "1":
    log(f"\nSKIP_TEARDOWN=1: QEMU + bmcweb-ng left running.")
    log(f"  bmcweb-ng : http://{HOST}:{NG_PORT}/redfish/v1")
    log(f"  upstream  : https://{HOST}:{HTTPS}/redfish/v1")
    log("  Waiting for parent process to kill QEMU (send SIGTERM to this process).")
    try:
        proc.wait()
    except KeyboardInterrupt:
        proc.terminate()
    sys.exit(0)   # skip summary + sys.exit(1) below

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
