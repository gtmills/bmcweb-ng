#!/usr/bin/env python3
"""
Launch rainier-bmc QEMU and wait for bmcweb to come up.
Prints last N boot-log lines every 30s for visibility.
"""
import subprocess, time, sys, os

_REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
IMGDIR = os.path.join(_REPO_ROOT, "target", "qemu-test", "rainier-image")
LOG    = "/tmp/rainier_qemu.log"
HOST   = "127.0.0.1"
PORT   = 2443
TIMEOUT = 600  # seconds to wait for bmcweb (allows for watchdog reboot on QEMU)

# Kill stale QEMU
subprocess.run(["pkill", "-f", "qemu-system-arm"], capture_output=True)
time.sleep(1)

cmd = [
    "qemu-system-arm",
    "-M", "rainier-bmc",
    "-nographic",
    "-kernel",  f"{IMGDIR}/zImage",
    "-dtb",     f"{IMGDIR}/aspeed-bmc-ibm-rainier.dtb",
    "-initrd",  f"{IMGDIR}/obmc-phosphor-initramfs.rootfs.cpio.xz",
    "-drive",   f"file={IMGDIR}/obmc-phosphor-image.rootfs.wic.qcow2,if=sd,index=2,snapshot=on",
    "-append",  "console=ttyS4,115200n8 rootwait root=PARTLABEL=rofs-a systemd.watchdog-device= nowatchdog",
    "-net", "nic",
    "-net", "user,hostfwd=tcp::2443-:443,hostfwd=tcp::2080-:80,hostfwd=tcp::2222-:22",
]

print(f"Launching QEMU (log -> {LOG})...")
with open(LOG, "w") as lf:
    proc = subprocess.Popen(cmd, stdout=lf, stderr=lf)
print(f"QEMU pid={proc.pid}")

deadline = time.time() + TIMEOUT
last_line = 0
next_print = time.time() + 20

while time.time() < deadline:
    # Check QEMU died
    if proc.poll() is not None:
        print(f"\nQEMU exited with code {proc.returncode}")
        try:
            lines = open(LOG).readlines()
            print("".join(lines[-20:]))
        except Exception:
            pass
        sys.exit(1)

    # Periodic log dump
    if time.time() >= next_print:
        try:
            lines = open(LOG).readlines()
            new = lines[last_line:]
            if new:
                print(f"[{int(time.time()-deadline+TIMEOUT)}s] --- log lines {last_line}-{len(lines)} ---")
                print("".join(new[-30:]), end="")
                last_line = len(lines)
        except Exception:
            pass
        next_print = time.time() + 30

    # Poll HTTPS
    r = subprocess.run(
        ["curl", "-sk", "--max-time", "4",
         f"https://{HOST}:{PORT}/redfish/v1",
         "-o", "/dev/null", "-w", "%{http_code}"],
        capture_output=True, text=True
    )
    code = r.stdout.strip()
    if code in ("200", "401"):
        elapsed = int(time.time() - deadline + TIMEOUT)
        print(f"\n[READY] bmcweb up — HTTP {code} after {elapsed}s")
        with open("/tmp/rainier_qemu.pid", "w") as f:
            f.write(str(proc.pid))
        sys.exit(0)

    time.sleep(5)

proc.terminate()
print(f"\n[TIMEOUT] bmcweb did not respond in {TIMEOUT}s")
try:
    lines = open(LOG).readlines()
    print("Last 30 lines:")
    print("".join(lines[-30:]))
except Exception:
    pass
sys.exit(1)
