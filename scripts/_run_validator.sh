#!/usr/bin/env bash
# _run_validator.sh
#
# Boots rainier-bmc QEMU, injects bmcweb-ng, runs the DMTF Redfish Service
# Validator against the live bmcweb-ng instance, then terminates QEMU.
#
# All paths are derived from the script's own location — no hardcoded user paths.
#
# Usage:
#   bash scripts/_run_validator.sh
#
# Requirements:
#   - qemu-system-arm with rainier-bmc machine support (>= 7.1)
#   - Built ARM binary: cargo build --release --target arm-unknown-linux-gnueabihf
#   - Python deps: pip install redfish requests colorama

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
# The validator lives three levels up from scripts/ (ai/dmtf/...)
_VALIDATOR_DIR="$(cd "${SCRIPT_DIR}/../../../dmtf/Redfish-Service-Validator" 2>/dev/null && pwd || echo "")"
VALIDATOR="${_VALIDATOR_DIR}/RedfishServiceValidator.py"

# Allow override via env
VALIDATOR_SCRIPT="${REDFISH_VALIDATOR:-${VALIDATOR}}"
LOG_DIR="${VALIDATOR_LOG_DIR:-/tmp/redfish_validator_logs}"
NG_PORT=8080
NG_HOST="127.0.0.1"
ADMIN_USER="${BMCWEB_USER:-admin}"
ADMIN_PASS="${BMCWEB_PASS:-0penBmc2!}"

mkdir -p "${LOG_DIR}"

if [[ ! -f "${VALIDATOR_SCRIPT}" ]]; then
    echo "ERROR: RedfishServiceValidator.py not found at: ${VALIDATOR_SCRIPT}"
    echo "Set REDFISH_VALIDATOR=/path/to/RedfishServiceValidator.py to override."
    exit 1
fi

echo ""
echo "============================================================"
echo "  DMTF Redfish Service Validator — bmcweb-ng"
echo "============================================================"
echo "  Repo root : ${REPO_ROOT}"
echo "  Validator : ${VALIDATOR_SCRIPT}"
echo "  Log dir   : ${LOG_DIR}"
echo ""

# ── Step 1: Kill any leftover QEMU, start fresh ───────────────────────────────
pkill -f qemu-system-arm 2>/dev/null || true
rm -f /tmp/rainier-serial.sock
sleep 1

# ── Step 2: Run the e2e boot+inject script (background) ──────────────────────
# _e2e_test.py boots QEMU, sets credentials, and injects bmcweb-ng.
# When SKIP_TEARDOWN=1 it does NOT terminate QEMU at the end, leaving it
# running for us to run the validator against.
echo ">>> Booting QEMU and injecting bmcweb-ng (background)..."
SKIP_TEARDOWN=1 python3 "${SCRIPT_DIR}/_e2e_test.py" &
E2E_PID=$!

# ── Step 3: Wait for bmcweb-ng on HTTP 8080 ──────────────────────────────────
echo ">>> Waiting for bmcweb-ng on http://${NG_HOST}:${NG_PORT} ..."
DEADLINE=$((SECONDS + 1200))
NG_UP=false
while [[ ${SECONDS} -lt ${DEADLINE} ]]; do
    CODE=$(curl -s --max-time 3 -o /dev/null -w '%{http_code}' \
        "http://${NG_HOST}:${NG_PORT}/redfish/v1" 2>/dev/null || echo "000")
    if [[ "${CODE}" == "200" || "${CODE}" == "401" ]]; then
        NG_UP=true
        echo "  bmcweb-ng is up (HTTP ${CODE})"
        break
    fi
    sleep 5
done

if [[ "${NG_UP}" != "true" ]]; then
    echo "ERROR: bmcweb-ng did not come up within timeout"
    kill "${E2E_PID}" 2>/dev/null || true
    pkill -f qemu-system-arm 2>/dev/null || true
    exit 1
fi

# ── Step 4: Run the DMTF Redfish Service Validator ───────────────────────────
echo ""
echo ">>> Running DMTF Redfish Service Validator..."
echo "    Target  : http://${NG_HOST}:${NG_PORT}"
echo "    Log dir : ${LOG_DIR}"
echo ""

python3 "${VALIDATOR_SCRIPT}" \
    --rhost "http://${NG_HOST}:${NG_PORT}" \
    --user "${ADMIN_USER}" \
    --password "${ADMIN_PASS}" \
    --authtype Basic \
    --logdir "${LOG_DIR}" \
    --nooemcheck \
    --timeout 30 \
    --collectionlimit LogEntry 5 Sensor 5 \
    2>&1 | tee "${LOG_DIR}/validator_run.log"

VALIDATOR_RC=${PIPESTATUS[0]}

# ── Step 5: Kill e2e background process + QEMU ───────────────────────────────
echo ""
echo ">>> Stopping QEMU..."
kill "${E2E_PID}" 2>/dev/null || true
pkill -f qemu-system-arm 2>/dev/null || true

echo ""
echo "============================================================"
echo "  Results saved to : ${LOG_DIR}/"
echo "  Validator exit   : ${VALIDATOR_RC}"
echo "============================================================"

exit ${VALIDATOR_RC}
