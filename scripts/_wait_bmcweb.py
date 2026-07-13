#!/usr/bin/env python3
"""Poll the QEMU boot log and wait for bmcweb to come up on HTTPS :2443."""
import time, subprocess, sys, os

LOG    = "/tmp/rainier_qemu.log"
HOST   = "127.0.0.1"
PORT   = 2443
TIMEOUT = 300   # seconds

print(f"Polling log and https://{HOST}:{PORT}/redfish/v1 (up to {TIMEOUT}s)...")
deadline = time.time() + TIMEOUT
last_lines = 0

while time.time() < deadline:
    # show new log lines
    try:
        lines = open(LOG).readlines()
        if len(lines) > last_lines:
            for l in lines[last_lines:]:
                sys.stdout.write(l)
            sys.stdout.flush()
            last_lines = len(lines)
    except Exception:
        pass

    # try HTTPS
    r = subprocess.run(
        ["curl", "-sk", "--max-time", "4",
         f"https://{HOST}:{PORT}/redfish/v1", "-o", "/dev/null", "-w", "%{http_code}"],
        capture_output=True, text=True
    )
    code = r.stdout.strip()
    if code in ("200", "401"):
        print(f"\n[READY] bmcweb responded HTTP {code} after ~{int(time.time()-deadline+TIMEOUT)}s")
        sys.exit(0)

    time.sleep(5)

print(f"\n[TIMEOUT] bmcweb did not respond within {TIMEOUT}s")
print("Last 20 log lines:")
try:
    lines = open(LOG).readlines()
    for l in lines[-20:]:
        sys.stdout.write(l)
except Exception as e:
    print(f"log error: {e}")
sys.exit(1)
