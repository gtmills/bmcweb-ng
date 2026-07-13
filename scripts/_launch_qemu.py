#!/usr/bin/env python3
"""Launch rainier-bmc QEMU in the background."""
import subprocess, os, time, sys

_REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
IMGDIR = os.path.join(_REPO_ROOT, "target", "qemu-test", "rainier-image")
LOG    = "/tmp/rainier_qemu.log"
PID_F  = "/tmp/rainier_qemu.pid"

# Kill any stale instance
subprocess.run(["pkill", "-f", "qemu-system-arm.*rainier"], capture_output=True)
time.sleep(1)

cmd = [
    "qemu-system-arm",
    "-M", "rainier-bmc",
    "-nographic",
    "-kernel",  f"{IMGDIR}/zImage",
    "-dtb",     f"{IMGDIR}/aspeed-bmc-ibm-rainier.dtb",
    "-initrd",  f"{IMGDIR}/obmc-phosphor-initramfs.rootfs.cpio.xz",
    "-drive",   f"file={IMGDIR}/obmc-phosphor-image.rootfs.wic.qcow2,if=sd,index=2,snapshot=on",
    "-append",  "console=ttyS4,115200n8 rootwait root=PARTLABEL=rofs-a",
    "-net", "nic",
    "-net", "user,hostfwd=tcp::2443-:443,hostfwd=tcp::2080-:80,hostfwd=tcp::2222-:22",
]

with open(LOG, "w") as log_fh:
    proc = subprocess.Popen(cmd, stdout=log_fh, stderr=log_fh)

with open(PID_F, "w") as f:
    f.write(str(proc.pid))

print(f"QEMU started, pid={proc.pid}, log={LOG}")
print("Waiting 12s for initial boot lines...")
time.sleep(12)

# Print first 50 lines of log
print("=== boot log (first 50 lines) ===")
try:
    lines = open(LOG).readlines()
    for l in lines[:50]:
        print(l, end="")
    print(f"... ({len(lines)} lines total so far)")
except Exception as e:
    print(f"log read error: {e}")
