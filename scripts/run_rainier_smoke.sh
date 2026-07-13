#!/usr/bin/env bash
# run_rainier_smoke.sh — boot rainier-bmc QEMU, run Redfish smoke tests
# against both upstream bmcweb and bmcweb-ng, then print pass/fail summary.
#
# Usage (from WSL2):
#   bash scripts/run_rainier_smoke.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
IMGDIR="${REPO_ROOT}/target/qemu-test/rainier-image"
BMCWEB_NG="${REPO_ROOT}/target/arm-unknown-linux-gnueabihf/release/bmcwebd-ng"
BMC_HOST="127.0.0.1"
BMC_HTTPS_PORT="2443"   # host-side port forwarded to VM :443
BMC_HTTP_PORT="2080"    # host-side port forwarded to VM :80 (bmcweb-ng)
SSH_PORT="2222"         # host-side port forwarded to VM :22
BOOT_TIMEOUT=180        # seconds to wait for bmcweb to respond
QEMU_PID_FILE="/tmp/rainier_qemu.pid"
LOG="/tmp/rainier_qemu.log"

# ── helpers ──────────────────────────────────────────────────────────────────
info()  { echo "[INFO]  $*"; }
pass()  { echo "[PASS]  $*"; }
fail()  { echo "[FAIL]  $*"; FAILURES=$((FAILURES+1)); }
FAILURES=0

cleanup() {
    if [ -f "$QEMU_PID_FILE" ]; then
        PID=$(cat "$QEMU_PID_FILE")
        kill "$PID" 2>/dev/null || true
        rm -f "$QEMU_PID_FILE"
        info "QEMU stopped (pid $PID)"
    fi
}
trap cleanup EXIT

# ── 1. start QEMU ─────────────────────────────────────────────────────────────
info "Starting rainier-bmc QEMU..."
qemu-system-arm \
    -M rainier-bmc,boot-emmc=false \
    -nographic \
    -kernel  "${IMGDIR}/zImage" \
    -dtb     "${IMGDIR}/aspeed-bmc-ibm-rainier.dtb" \
    -initrd  "${IMGDIR}/obmc-phosphor-initramfs.rootfs.cpio.xz" \
    -drive   "file=${IMGDIR}/obmc-phosphor-image.rootfs.wic.qcow2,if=sd,index=2,snapshot=on" \
    -append  "console=ttyS4,115200n8 rootwait root=PARTLABEL=rofs-a" \
    -net nic \
    -net "user,hostfwd=tcp::${BMC_HTTPS_PORT}-:443,hostfwd=tcp::${BMC_HTTP_PORT}-:80,hostfwd=tcp::${SSH_PORT}-:22" \
    > "$LOG" 2>&1 &
echo $! > "$QEMU_PID_FILE"
info "QEMU pid $(cat $QEMU_PID_FILE), log: $LOG"

# ── 2. wait for bmcweb (upstream, HTTPS :443 → host :2443) ────────────────────
info "Waiting up to ${BOOT_TIMEOUT}s for upstream bmcweb on https://${BMC_HOST}:${BMC_HTTPS_PORT}..."
DEADLINE=$((SECONDS + BOOT_TIMEOUT))
while [ $SECONDS -lt $DEADLINE ]; do
    if curl -sk --max-time 3 "https://${BMC_HOST}:${BMC_HTTPS_PORT}/redfish/v1" -o /dev/null 2>/dev/null; then
        info "bmcweb is up after ~$((SECONDS - (DEADLINE - BOOT_TIMEOUT)))s"
        break
    fi
    sleep 5
done
if [ $SECONDS -ge $DEADLINE ]; then
    fail "bmcweb did not come up within ${BOOT_TIMEOUT}s"
    exit 1
fi

# ── 3. baseline smoke tests — upstream bmcweb (HTTPS) ─────────────────────────
info "=== Smoke tests: upstream bmcweb ==="
BASE="https://${BMC_HOST}:${BMC_HTTPS_PORT}"
CREDS="-u root:0penBmc"   # default OpenBMC credentials

run_test() {
    local name="$1" url="$2" field="$3" expected="$4"
    RESP=$(curl -sk --max-time 10 $CREDS "$url" 2>/dev/null)
    ACTUAL=$(echo "$RESP" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d$field)" 2>/dev/null || echo "__ERROR__")
    if [ "$ACTUAL" = "$expected" ]; then
        pass "$name"
    else
        fail "$name  (got: '$ACTUAL', want: '$expected')"
    fi
}

run_test "GET /redfish/v1 → RedfishVersion"     "${BASE}/redfish/v1"                   "['RedfishVersion']"  "1.6.0"
run_test "GET /redfish/v1/Systems"               "${BASE}/redfish/v1/Systems"            "['@odata.type']"     "#ComputerSystemCollection.ComputerSystemCollection"
run_test "GET /redfish/v1/Chassis"               "${BASE}/redfish/v1/Chassis"            "['@odata.type']"     "#ChassisCollection.ChassisCollection"
run_test "GET /redfish/v1/Managers"              "${BASE}/redfish/v1/Managers"           "['@odata.type']"     "#ManagerCollection.ManagerCollection"
run_test "GET /redfish/v1/AccountService"        "${BASE}/redfish/v1/AccountService"     "['@odata.type']"     "#AccountService.v1_10_0.AccountService"

BASELINE_FAILURES=$FAILURES

# ── 4. inject bmcweb-ng, start on port 80 (HTTP) ──────────────────────────────
info "=== Injecting bmcweb-ng binary into VM ==="
# Wait for SSH to be ready
SSH_DEADLINE=$((SECONDS + 60))
while [ $SECONDS -lt $SSH_DEADLINE ]; do
    if ssh -o StrictHostKeyChecking=no -o ConnectTimeout=3 -p "$SSH_PORT" root@"$BMC_HOST" 'echo SSH_OK' 2>/dev/null | grep -q SSH_OK; then
        break
    fi
    sleep 3
done

scp -o StrictHostKeyChecking=no -P "$SSH_PORT" "$BMCWEB_NG" root@"${BMC_HOST}":/tmp/bmcwebd-ng 2>/dev/null
ssh -o StrictHostKeyChecking=no -p "$SSH_PORT" root@"$BMC_HOST" 'chmod +x /tmp/bmcwebd-ng' 2>/dev/null

info "Stopping upstream bmcweb..."
ssh -o StrictHostKeyChecking=no -p "$SSH_PORT" root@"$BMC_HOST" \
    'systemctl stop bmcweb 2>/dev/null; sleep 1' 2>/dev/null || true

info "Starting bmcweb-ng on port 80..."
ssh -o StrictHostKeyChecking=no -p "$SSH_PORT" root@"$BMC_HOST" \
    'nohup /tmp/bmcwebd-ng > /tmp/bmcwebd-ng.log 2>&1 &' 2>/dev/null

# Wait for bmcweb-ng to answer on HTTP :80 → host :2080
info "Waiting for bmcweb-ng on http://${BMC_HOST}:${BMC_HTTP_PORT}..."
NG_DEADLINE=$((SECONDS + 60))
while [ $SECONDS -lt $NG_DEADLINE ]; do
    if curl -s --max-time 3 "http://${BMC_HOST}:${BMC_HTTP_PORT}/redfish/v1" -o /dev/null 2>/dev/null; then
        info "bmcweb-ng is up"
        break
    fi
    sleep 3
done

# ── 5. smoke tests — bmcweb-ng (HTTP) ─────────────────────────────────────────
info "=== Smoke tests: bmcweb-ng ==="
NG_BASE="http://${BMC_HOST}:${BMC_HTTP_PORT}"

run_test_ng() {
    local name="$1" url="$2" field="$3" expected="$4"
    RESP=$(curl -s --max-time 10 "$url" 2>/dev/null)
    ACTUAL=$(echo "$RESP" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d$field)" 2>/dev/null || echo "__ERROR__")
    if [ "$ACTUAL" = "$expected" ]; then
        pass "$name"
    else
        fail "$name  (got: '$ACTUAL', want: '$expected')"
    fi
}

run_test_ng "GET /redfish/v1 → status 200"        "${NG_BASE}/redfish/v1"         "['RedfishVersion']"  "1.6.0"
run_test_ng "GET /redfish/v1/Systems collection"   "${NG_BASE}/redfish/v1/Systems" "['@odata.type']"     "#ComputerSystemCollection.ComputerSystemCollection"
run_test_ng "GET /redfish/v1/Chassis collection"   "${NG_BASE}/redfish/v1/Chassis" "['@odata.type']"     "#ChassisCollection.ChassisCollection"
run_test_ng "GET /redfish/v1/Managers collection"  "${NG_BASE}/redfish/v1/Managers" "['@odata.type']"    "#ManagerCollection.ManagerCollection"

# ── 6. summary ────────────────────────────────────────────────────────────────
echo ""
echo "════════════════════════════════════════"
echo " SMOKE TEST SUMMARY"
echo "════════════════════════════════════════"
echo " Baseline failures (upstream bmcweb): $BASELINE_FAILURES"
echo " Total failures (all tests):          $FAILURES"
if [ "$FAILURES" -eq 0 ]; then
    echo " RESULT: ALL TESTS PASSED ✓"
else
    echo " RESULT: $FAILURES TEST(S) FAILED ✗"
fi
echo "════════════════════════════════════════"

exit $FAILURES
