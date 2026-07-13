#!/usr/bin/env python3
"""
Run Redfish smoke tests against:
  1. Upstream bmcweb  (HTTPS :2443)
  2. bmcweb-ng         (HTTP  :2080, after injection)
"""
import subprocess, sys, json, time, os

_REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))

HOST      = "127.0.0.1"
HTTPS_PORT = 2443
HTTP_PORT  = 2080
SSH_PORT   = 2222
BMCWEB_NG  = os.path.join(_REPO_ROOT, "target", "arm-unknown-linux-gnueabihf", "release", "bmcwebd-ng")
CREDS      = ("root", "0penBmc")

PASS = 0
FAIL = 0

# ── helpers ──────────────────────────────────────────────────────────────────
def get(url, auth=None, tls=True):
    cmd = ["curl", "-s", "--max-time", "10"]
    if tls:
        cmd += ["-k"]
    if auth:
        cmd += ["-u", f"{auth[0]}:{auth[1]}"]
    cmd.append(url)
    r = subprocess.run(cmd, capture_output=True, text=True)
    try:
        return json.loads(r.stdout)
    except Exception:
        return {"__raw__": r.stdout, "__stderr__": r.stderr}

def check(name, val, expected):
    global PASS, FAIL
    if val == expected:
        print(f"  PASS  {name}")
        PASS += 1
    else:
        print(f"  FAIL  {name}")
        print(f"         got:    {repr(val)}")
        print(f"         expect: {repr(expected)}")
        FAIL += 1

def section(title):
    print(f"\n{'='*60}")
    print(f"  {title}")
    print(f"{'='*60}")

# ── 1. Upstream bmcweb (HTTPS) ────────────────────────────────────────────────
section("Upstream bmcweb  https://127.0.0.1:2443")

base = f"https://{HOST}:{HTTPS_PORT}"

d = get(f"{base}/redfish/v1", auth=CREDS)
check("GET /redfish/v1  → RedfishVersion == '1.6.0'",
      d.get("RedfishVersion"), "1.6.0")
check("GET /redfish/v1  → @odata.type contains 'ServiceRoot'",
      "ServiceRoot" in d.get("@odata.type", ""), True)

d = get(f"{base}/redfish/v1/Systems", auth=CREDS)
check("GET /redfish/v1/Systems  → @odata.type contains ComputerSystemCollection",
      "ComputerSystemCollection" in d.get("@odata.type", ""), True)

d = get(f"{base}/redfish/v1/Chassis", auth=CREDS)
check("GET /redfish/v1/Chassis  → @odata.type contains ChassisCollection",
      "ChassisCollection" in d.get("@odata.type", ""), True)

d = get(f"{base}/redfish/v1/Managers", auth=CREDS)
check("GET /redfish/v1/Managers  → @odata.type contains ManagerCollection",
      "ManagerCollection" in d.get("@odata.type", ""), True)

d = get(f"{base}/redfish/v1/AccountService", auth=CREDS)
check("GET /redfish/v1/AccountService  → @odata.type contains AccountService",
      "AccountService" in d.get("@odata.type", ""), True)

baseline_fail = FAIL

# ── 2. Inject bmcweb-ng ───────────────────────────────────────────────────────
section("Injecting bmcweb-ng via SCP/SSH")

ssh_opts = ["-o", "StrictHostKeyChecking=no", "-o", "ConnectTimeout=10", "-p", str(SSH_PORT)]

# Wait for SSH
print("  Waiting for SSH...")
deadline = time.time() + 60
ok = False
while time.time() < deadline:
    r = subprocess.run(
        ["ssh"] + ssh_opts + [f"root@{HOST}", "echo SSH_OK"],
        capture_output=True, text=True, timeout=12
    )
    if "SSH_OK" in r.stdout:
        ok = True
        break
    time.sleep(3)

if not ok:
    print("  FAIL  SSH not reachable — skipping bmcweb-ng tests")
    FAIL += 1
else:
    print("  SSH up")

    # SCP binary
    r = subprocess.run(
        ["scp"] + ["-o", "StrictHostKeyChecking=no", "-P", str(SSH_PORT),
                   BMCWEB_NG, f"root@{HOST}:/tmp/bmcwebd-ng"],
        capture_output=True, text=True, timeout=60
    )
    if r.returncode != 0:
        print(f"  FAIL  SCP: {r.stderr.strip()}")
        FAIL += 1
    else:
        print("  SCP OK (5.4 MB ARM binary uploaded)")

        # Stop upstream bmcweb
        subprocess.run(
            ["ssh"] + ssh_opts + [f"root@{HOST}", "systemctl stop bmcweb 2>/dev/null; sleep 1"],
            capture_output=True, timeout=15
        )
        print("  Stopped upstream bmcweb")

        # Start bmcweb-ng on port 80 (plain HTTP)
        subprocess.run(
            ["ssh"] + ssh_opts + [f"root@{HOST}",
             "chmod +x /tmp/bmcwebd-ng && nohup /tmp/bmcwebd-ng >/tmp/bmcwebd-ng.log 2>&1 &"],
            capture_output=True, timeout=15
        )
        print("  Started bmcweb-ng")

        # Wait for it
        print("  Waiting for bmcweb-ng on HTTP port 2080...")
        deadline2 = time.time() + 60
        ng_up = False
        while time.time() < deadline2:
            rc = subprocess.run(
                ["curl", "-s", "--max-time", "4", "-o", "/dev/null", "-w", "%{http_code}",
                 f"http://{HOST}:{HTTP_PORT}/redfish/v1"],
                capture_output=True, text=True
            )
            if rc.stdout.strip() in ("200", "401"):
                ng_up = True
                print(f"  bmcweb-ng answered HTTP {rc.stdout.strip()}")
                break
            time.sleep(3)

        if not ng_up:
            # Show remote log
            r2 = subprocess.run(
                ["ssh"] + ssh_opts + [f"root@{HOST}", "cat /tmp/bmcwebd-ng.log 2>/dev/null | tail -20"],
                capture_output=True, text=True, timeout=12
            )
            print(f"  bmcweb-ng log:\n{r2.stdout}")
            print("  FAIL  bmcweb-ng did not come up")
            FAIL += 1
        else:
            # ── 3. bmcweb-ng smoke tests (HTTP) ──────────────────────────────
            section("bmcweb-ng  http://127.0.0.1:2080")
            ng = f"http://{HOST}:{HTTP_PORT}"

            d = get(f"{ng}/redfish/v1", tls=False)
            check("GET /redfish/v1  → RedfishVersion present",
                  isinstance(d.get("RedfishVersion"), str), True)
            check("GET /redfish/v1  → @odata.type contains ServiceRoot",
                  "ServiceRoot" in d.get("@odata.type", ""), True)

            d = get(f"{ng}/redfish/v1/Systems", tls=False)
            check("GET /redfish/v1/Systems  → @odata.type contains Collection",
                  "Collection" in d.get("@odata.type", ""), True)

            d = get(f"{ng}/redfish/v1/Chassis", tls=False)
            check("GET /redfish/v1/Chassis  → @odata.type contains Collection",
                  "Collection" in d.get("@odata.type", ""), True)

            d = get(f"{ng}/redfish/v1/Managers", tls=False)
            check("GET /redfish/v1/Managers  → @odata.type contains Collection",
                  "Collection" in d.get("@odata.type", ""), True)

# ── Summary ────────────────────────────────────────────────────────────────────
section("SUMMARY")
print(f"  Baseline (upstream bmcweb) failures : {baseline_fail}")
print(f"  Total PASS : {PASS}")
print(f"  Total FAIL : {FAIL}")
if FAIL == 0:
    print("\n  *** ALL TESTS PASSED ***")
    sys.exit(0)
else:
    print(f"\n  *** {FAIL} TEST(S) FAILED ***")
    sys.exit(1)
