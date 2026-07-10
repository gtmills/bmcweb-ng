#!/usr/bin/env bash
# setup_qemu_test.sh
# Download the OpenBMC QEMU binary and a qemuarm OpenBMC image,
# boot OpenBMC in QEMU, and run a basic Redfish smoke test against
# the running bmcweb inside the VM.
#
# This script tests the *upstream* bmcweb that ships with OpenBMC qemuarm,
# which validates that our Redfish API surface matches expectations.
# A second phase builds bmcweb-ng cross-compiled for ARM and swaps it in.
#
# Usage:
#   bash scripts/setup_qemu_test.sh           # full flow: download + boot + test
#   SKIP_BOOT=1 bash scripts/setup_qemu_test.sh  # skip download/boot, only run tests
#                                                  # (VM must already be running)
#
# Requirements (installed automatically if missing):
#   - wget, curl, jq, zstd, ssh, sshpass

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
WORK_DIR="${REPO_DIR}/target/qemu-test"
QEMU_BINARY="${WORK_DIR}/qemu-system-arm"
QEMU_IMG_DIR="${WORK_DIR}/image"

QEMU_URL="https://jenkins.openbmc.org/job/latest-qemu-x86/lastSuccessfulBuild/artifact/qemu/build/qemu-system-arm"

# OpenBMC qemuarm image artifacts (latest successful build).
OPENBMC_JENKINS_BASE="https://jenkins.openbmc.org/job/ci-openbmc/job/openbmc/job/main/lastSuccessfulBuild"

# BMC credentials (default on OpenBMC)
BMC_USER="root"
BMC_PASS="0penBmc"
BMC_PORT=2443   # Host port forwarded to guest port 443
SSH_PORT=2222   # Host port forwarded to guest port 22

# ── colours ──────────────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'
info()  { echo -e "${GREEN}[INFO]${NC}  $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }

# ── helpers ───────────────────────────────────────────────────────────────────

need_cmd() {
    local pkg="${2:-$1}"  # optional package name override
    if ! command -v "$1" &>/dev/null; then
        warn "Installing $pkg..."
        sudo apt-get install -y "$pkg" >/dev/null 2>&1 || {
            error "Cannot install $pkg. Please install it manually."; exit 1
        }
    fi
}

# ── step 0: prerequisites ─────────────────────────────────────────────────────

info "Checking prerequisites..."
need_cmd wget
need_cmd curl
need_cmd jq
need_cmd zstd
need_cmd ssh openssh-client
need_cmd sshpass

mkdir -p "${WORK_DIR}" "${QEMU_IMG_DIR}"

# ── SKIP_BOOT: jump straight to tests if VM is already running ────────────────

if [[ "${SKIP_BOOT:-0}" == "1" ]]; then
    info "SKIP_BOOT=1: skipping download and VM boot, running smoke tests only."
    PASS=0; FAIL=0; RESULTS=()
    # shellcheck disable=SC2317
    # (functions defined later — sourced by the skip path)
else

# ── step 1: download QEMU binary from OpenBMC Jenkins ─────────────────────────

if [[ ! -f "${QEMU_BINARY}" ]]; then
    info "Downloading QEMU binary from OpenBMC Jenkins..."
    info "  URL: ${QEMU_URL}"
    wget -q --show-progress -O "${QEMU_BINARY}" "${QEMU_URL}" || {
        error "Failed to download QEMU binary from ${QEMU_URL}"
        error "Check your network or see QEMU_SETUP.md for manual steps."
        exit 1
    }
    chmod +x "${QEMU_BINARY}"
    info "QEMU downloaded: $(du -sh "${QEMU_BINARY}" | cut -f1)"
else
    info "QEMU binary already present: ${QEMU_BINARY}"
fi

# Verify the binary can execute
if ! "${QEMU_BINARY}" --version 2>/dev/null | grep -q "QEMU"; then
    error "Downloaded QEMU binary does not execute correctly or is not a QEMU binary."
    file "${QEMU_BINARY}" 2>/dev/null || true
    exit 1
fi
info "QEMU version: $("${QEMU_BINARY}" --version 2>/dev/null | head -1)"

# ── step 2: download OpenBMC qemuarm image ────────────────────────────────────

KERNEL="${QEMU_IMG_DIR}/uImage"
ROOTFS="${QEMU_IMG_DIR}/obmc-phosphor-image-qemuarm.ext4"
DTB="${QEMU_IMG_DIR}/qemuarm.dtb"

download_openbmc_image() {
    local base_url="${OPENBMC_JENKINS_BASE}"

    info "Probing OpenBMC Jenkins for qemuarm artifacts..."

    # Fetch the build artifact manifest from Jenkins JSON API
    local api_url="${base_url}/api/json?tree=artifacts[relativePath,fileName]"
    local manifest
    manifest=$(curl -sf --max-time 30 "${api_url}" 2>/dev/null || echo "")

    if [[ -z "${manifest}" ]]; then
        warn "Jenkins API did not respond — trying direct URL patterns."
    fi

    local kernel_path="" rootfs_path="" dtb_path=""

    if [[ -n "${manifest}" ]]; then
        kernel_path=$(echo "${manifest}" | jq -r \
            '.artifacts[].relativePath | select(test("qemuarm.*uImage$"))' \
            2>/dev/null | head -1)
        rootfs_path=$(echo "${manifest}" | jq -r \
            '.artifacts[].relativePath | select(test("qemuarm.*ext4\\.zst$"))' \
            2>/dev/null | head -1)
        dtb_path=$(echo "${manifest}" | jq -r \
            '.artifacts[].relativePath | select(test("qemuarm.*\\.dtb$"))' \
            2>/dev/null | head -1)
    fi

    local artifact_base="${base_url}/artifact"

    # ── kernel ────────────────────────────────────────────────────────────────
    if [[ -n "${kernel_path}" ]]; then
        info "Downloading kernel: ${kernel_path}"
        wget -q --show-progress -O "${KERNEL}" \
            "${artifact_base}/${kernel_path}" || {
            error "Failed to download kernel"; return 1
        }
    else
        # Fallback: well-known path pattern
        local kernel_url="${artifact_base}/openbmc/build/tmp/deploy/images/qemuarm/uImage"
        info "Kernel not found in manifest, trying well-known URL: ${kernel_url}"
        wget -q --show-progress -O "${KERNEL}" "${kernel_url}" 2>/dev/null || true
    fi

    # ── rootfs ────────────────────────────────────────────────────────────────
    if [[ -n "${rootfs_path}" ]]; then
        info "Downloading rootfs: ${rootfs_path}"
        local zst_file="${ROOTFS}.zst"
        wget -q --show-progress -O "${zst_file}" \
            "${artifact_base}/${rootfs_path}" || {
            error "Failed to download rootfs"; return 1
        }
        info "Decompressing rootfs..."
        zstd -d "${zst_file}" -o "${ROOTFS}" --force
        rm -f "${zst_file}"
    else
        warn "Rootfs not found in Jenkins manifest. See QEMU_SETUP.md for manual download."
    fi

    # ── DTB ───────────────────────────────────────────────────────────────────
    if [[ -n "${dtb_path}" ]]; then
        info "Downloading DTB: ${dtb_path}"
        wget -q --show-progress -O "${DTB}" \
            "${artifact_base}/${dtb_path}" || {
            error "Failed to download DTB"; return 1
        }
    else
        # Fallback: try the well-known single-name DTB
        local dtb_url="${artifact_base}/openbmc/build/tmp/deploy/images/qemuarm/qemuarm.dtb"
        info "DTB not found in manifest, trying well-known URL: ${dtb_url}"
        wget -q --show-progress -O "${DTB}" "${dtb_url}" 2>/dev/null || true
    fi

    # ── validate all three files are present ─────────────────────────────────
    local missing=0
    [[ -f "${KERNEL}" ]] || { error "Kernel missing after download: ${KERNEL}"; missing=1; }
    [[ -f "${ROOTFS}" ]] || { error "Rootfs missing after download: ${ROOTFS}"; missing=1; }
    [[ -f "${DTB}" ]]    || { error "DTB missing after download: ${DTB}";    missing=1; }

    if [[ "${missing}" -ne 0 ]]; then
        error ""
        error "One or more image files could not be downloaded."
        error "Manual steps:"
        error "  1. Go to ${OPENBMC_JENKINS_BASE}/"
        error "  2. Download from: openbmc/build/tmp/deploy/images/qemuarm/"
        error "       uImage  (kernel)"
        error "       obmc-phosphor-image-qemuarm-*.rootfs.ext4.zst  (rootfs)"
        error "       qemuarm-*.dtb  (device tree)"
        error "  3. Place decompressed files in: ${QEMU_IMG_DIR}/"
        error "     (run: zstd -d *.rootfs.ext4.zst)"
        error "  4. Re-run this script"
        return 1
    fi

    info "All image files downloaded successfully."
    info "  Kernel:  $(du -sh "${KERNEL}" | cut -f1)"
    info "  Rootfs:  $(du -sh "${ROOTFS}" | cut -f1)"
    info "  DTB:     $(du -sh "${DTB}" | cut -f1)"
}

if [[ ! -f "${KERNEL}" || ! -f "${ROOTFS}" || ! -f "${DTB}" ]]; then
    info "Downloading OpenBMC qemuarm image..."
    download_openbmc_image || {
        exit 1
    }
else
    info "OpenBMC qemuarm image already present (use 'rm -rf ${QEMU_IMG_DIR}' to re-download)."
fi

# ── step 3: check for port conflicts ──────────────────────────────────────────

check_port_free() {
    local port="$1"
    if ss -tlnp 2>/dev/null | grep -q ":${port} " || \
       netstat -tlnp 2>/dev/null | grep -q ":${port} "; then
        error "Port ${port} is already in use. Kill the conflicting process or"
        error "set BMC_PORT / SSH_PORT env vars to use different ports."
        exit 1
    fi
}

info "Checking for port conflicts..."
check_port_free "${BMC_PORT}"
check_port_free "${SSH_PORT}"
check_port_free "2080"

# ── step 4: boot OpenBMC in QEMU ──────────────────────────────────────────────

QEMU_PIDFILE="${WORK_DIR}/qemu.pid"
QEMU_LOG="${WORK_DIR}/qemu.log"

start_qemu() {
    info "Booting OpenBMC qemuarm in QEMU..."
    info "  Kernel:  ${KERNEL}"
    info "  Rootfs:  ${ROOTFS}"
    info "  DTB:     ${DTB}"

    # Make a writable copy of the rootfs so we can inject bmcweb-ng later
    # without corrupting the pristine image.
    local rw_rootfs="${WORK_DIR}/rootfs-rw.ext4"
    cp -f "${ROOTFS}" "${rw_rootfs}"

    # QEMU invocation matches the upstream OpenBMC run-qemu script:
    #   https://github.com/openbmc/openbmc/blob/main/scripts/run-qemu
    "${QEMU_BINARY}" \
        -machine          versatilepb \
        -m                256 \
        -drive            "file=${rw_rootfs},if=virtio,format=raw" \
        -net              "nic" \
        -net              "user,hostfwd=tcp::${BMC_PORT}-:443,hostfwd=tcp::${SSH_PORT}-:22,hostfwd=tcp::2080-:80" \
        -kernel           "${KERNEL}" \
        -dtb              "${DTB}" \
        -append           "root=/dev/vda rw console=ttyAMA0,115200 ignore_loglevel" \
        -display          none \
        -serial           "file:${QEMU_LOG}" \
        -pidfile          "${QEMU_PIDFILE}" \
        -daemonize \
        2>&1 || {
            error "QEMU failed to start. Check ${QEMU_LOG}"
            if [[ -f "${QEMU_LOG}" ]]; then
                echo "--- Last 20 lines of QEMU log ---"
                tail -20 "${QEMU_LOG}" 2>/dev/null || true
                echo "---------------------------------"
            fi
            return 1
        }

    local pid
    pid=$(cat "${QEMU_PIDFILE}" 2>/dev/null || echo "unknown")
    info "QEMU started (PID: ${pid})"
    info "  HTTPS forwarded to: localhost:${BMC_PORT}"
    info "  SSH forwarded to:   localhost:${SSH_PORT}"
    info "  Serial log: ${QEMU_LOG}"
    info "  (tail -f ${QEMU_LOG} to watch the boot)"
}

stop_qemu() {
    if [[ -f "${QEMU_PIDFILE}" ]]; then
        local pid
        pid=$(cat "${QEMU_PIDFILE}" 2>/dev/null || echo "")
        if [[ -n "${pid}" ]] && kill -0 "${pid}" 2>/dev/null; then
            info "Stopping QEMU (PID ${pid})..."
            kill "${pid}" 2>/dev/null || true
            sleep 2
        fi
        rm -f "${QEMU_PIDFILE}"
    fi
}

wait_for_bmc() {
    info "Waiting for OpenBMC to become available (up to 5 min)..."
    local retries=60
    local delay=5
    local i
    for ((i=0; i<retries; i++)); do
        local code
        code=$(curl -sk --max-time 5 \
            -u "${BMC_USER}:${BMC_PASS}" \
            -o /dev/null -w "%{http_code}" \
            "https://localhost:${BMC_PORT}/redfish/v1" 2>/dev/null)
        if [[ "${code}" == "200" ]]; then
            info "OpenBMC is up and responding to Redfish requests."
            return 0
        fi
        printf "."
        sleep "${delay}"
    done
    echo ""
    error "Timed out waiting for OpenBMC to start after $((retries * delay))s."
    error "Check the boot log: tail -f ${QEMU_LOG}"
    return 1
}

# Trap to ensure QEMU is stopped on EXIT (applies even with SKIP_BOOT=0 path)
trap 'stop_qemu' EXIT

start_qemu
wait_for_bmc

fi  # end of: if [[ "${SKIP_BOOT:-0}" != "1" ]]

# If SKIP_BOOT=1 we still need stop_qemu defined but as a no-op since we
# don't own the VM process.
if [[ "${SKIP_BOOT:-0}" == "1" ]]; then
    stop_qemu() { :; }
    trap 'stop_qemu' EXIT
fi

# ── step 5: Redfish smoke tests ───────────────────────────────────────────────

PASS=0
FAIL=0
RESULTS=()

# Combined curl call: get HTTP code and body in one request.
# Returns body in BMC_RESPONSE and HTTP code in BMC_HTTP_CODE.
BMC_RESPONSE=""
BMC_HTTP_CODE=""
_bmc_curl_get() {
    local url="$1"
    local tmpfile
    tmpfile=$(mktemp)
    BMC_HTTP_CODE=$(curl -sk --max-time 10 \
        -u "${BMC_USER}:${BMC_PASS}" \
        -H "Content-Type: application/json" \
        -o "${tmpfile}" -w "%{http_code}" \
        "${url}" 2>/dev/null)
    BMC_RESPONSE=$(cat "${tmpfile}")
    rm -f "${tmpfile}"
}

redfish_get() {
    local endpoint="$1"
    local expected_field="${2:-}"
    local expected_value="${3:-}"

    _bmc_curl_get "https://localhost:${BMC_PORT}${endpoint}"

    if [[ "${BMC_HTTP_CODE}" != "200" ]]; then
        FAIL=$((FAIL+1))
        RESULTS+=("FAIL  GET ${endpoint}  (HTTP ${BMC_HTTP_CODE})")
        return 1
    fi

    if [[ -n "${expected_field}" ]]; then
        local actual
        actual=$(echo "${BMC_RESPONSE}" | jq -r "${expected_field}" 2>/dev/null)
        if [[ "${actual}" == "${expected_value}" ]]; then
            PASS=$((PASS+1))
            RESULTS+=("PASS  GET ${endpoint}  (${expected_field} = ${expected_value})")
        else
            FAIL=$((FAIL+1))
            RESULTS+=("FAIL  GET ${endpoint}  (expected ${expected_field}=${expected_value}, got '${actual}')")
        fi
    else
        PASS=$((PASS+1))
        RESULTS+=("PASS  GET ${endpoint}")
    fi
}

redfish_post() {
    local endpoint="$1"
    local payload="$2"
    local expected_code="$3"

    local http_code
    http_code=$(curl -sk --max-time 10 \
        -X POST \
        -H "Content-Type: application/json" \
        -d "${payload}" \
        -o /dev/null -w "%{http_code}" \
        "https://localhost:${BMC_PORT}${endpoint}" 2>/dev/null)

    if [[ "${http_code}" == "${expected_code}" ]]; then
        PASS=$((PASS+1))
        RESULTS+=("PASS  POST ${endpoint}  (HTTP ${http_code})")
    else
        FAIL=$((FAIL+1))
        RESULTS+=("FAIL  POST ${endpoint}  (expected ${expected_code}, got ${http_code})")
    fi
}

run_smoke_tests() {
    info "Running Redfish smoke tests against https://localhost:${BMC_PORT} ..."

    # ServiceRoot
    redfish_get "/redfish/v1" '.RedfishVersion' "1.17.0"
    redfish_get "/redfish/v1" '."@odata.type"' "#ServiceRoot.v1_15_0.ServiceRoot"

    # Systems
    redfish_get "/redfish/v1/Systems" '."@odata.type"' "#ComputerSystemCollection.ComputerSystemCollection"
    redfish_get "/redfish/v1/Systems/system" '.Id' "system"
    redfish_get "/redfish/v1/Systems/system" '."@odata.type"' "#ComputerSystem.v1_20_0.ComputerSystem"

    # Chassis
    redfish_get "/redfish/v1/Chassis" '."@odata.type"' "#ChassisCollection.ChassisCollection"
    redfish_get "/redfish/v1/Chassis/chassis" '.Id' "chassis"

    # Managers
    redfish_get "/redfish/v1/Managers" '."@odata.type"' "#ManagerCollection.ManagerCollection"
    redfish_get "/redfish/v1/Managers/bmc" '.Id' "bmc"
    redfish_get "/redfish/v1/Managers/bmc" '.ManagerType' "BMC"

    # SessionService
    redfish_get "/redfish/v1/SessionService" '.ServiceEnabled' "true"
    redfish_post "/redfish/v1/SessionService/Sessions" \
        '{"UserName":"root","Password":"0penBmc"}' "201"

    # AccountService
    redfish_get "/redfish/v1/AccountService" '."@odata.type"' "#AccountService.v1_12_0.AccountService"
    redfish_get "/redfish/v1/AccountService/Accounts" '."@odata.type"' "#ManagerAccountCollection.ManagerAccountCollection"
    redfish_get "/redfish/v1/AccountService/Roles/Administrator" '.IsPredefined' "true"

    # TaskService
    redfish_get "/redfish/v1/TaskService" '.ServiceEnabled' "true"

    # UpdateService
    redfish_get "/redfish/v1/UpdateService" '.ServiceEnabled' "true"
    redfish_get "/redfish/v1/UpdateService/FirmwareInventory" '."@odata.type"' "#SoftwareInventoryCollection.SoftwareInventoryCollection"

    # EventService
    redfish_get "/redfish/v1/EventService" '.ServiceEnabled' "true"

    # NetworkProtocol
    redfish_get "/redfish/v1/Managers/bmc/NetworkProtocol" '."@odata.type"' "#ManagerNetworkProtocol.v1_9_0.ManagerNetworkProtocol"

    # EthernetInterfaces
    redfish_get "/redfish/v1/Managers/bmc/EthernetInterfaces" '."@odata.type"' "#EthernetInterfaceCollection.EthernetInterfaceCollection"

    # Chassis sub-resources
    redfish_get "/redfish/v1/Chassis/chassis/Power" '."@odata.type"' "#Power.v1_7_2.Power"
    redfish_get "/redfish/v1/Chassis/chassis/Thermal" '."@odata.type"' "#Thermal.v1_8_0.Thermal"

    # Auth enforcement — unauthenticated GET must return 401
    local unauth_code
    unauth_code=$(curl -sk --max-time 5 \
        -o /dev/null -w "%{http_code}" \
        "https://localhost:${BMC_PORT}/redfish/v1/Systems" 2>/dev/null)
    if [[ "${unauth_code}" == "401" ]]; then
        PASS=$((PASS+1))
        RESULTS+=("PASS  Unauthenticated GET /Systems returns 401")
    else
        FAIL=$((FAIL+1))
        RESULTS+=("FAIL  Unauthenticated GET /Systems (expected 401, got ${unauth_code})")
    fi
}

print_results() {
    echo ""
    echo "════════════════════════════════════════════════════════"
    echo "  Redfish Smoke Test Results"
    echo "════════════════════════════════════════════════════════"
    local r
    for r in "${RESULTS[@]}"; do
        if [[ "${r}" == PASS* ]]; then
            echo -e "  ${GREEN}${r}${NC}"
        else
            echo -e "  ${RED}${r}${NC}"
        fi
    done
    echo "────────────────────────────────────────────────────────"
    echo -e "  Total: $((PASS+FAIL))  |  ${GREEN}Pass: ${PASS}${NC}  |  ${RED}Fail: ${FAIL}${NC}"
    echo "════════════════════════════════════════════════════════"
    echo ""
}

# ── main ──────────────────────────────────────────────────────────────────────

run_smoke_tests
print_results

if [[ "${FAIL}" -gt 0 ]]; then
    error "Some smoke tests failed."
    [[ "${SKIP_BOOT:-0}" != "1" ]] && error "Boot log: ${QEMU_LOG}"
    exit 1
else
    info "All smoke tests passed."
fi
