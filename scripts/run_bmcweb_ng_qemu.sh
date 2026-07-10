#!/usr/bin/env bash
# run_bmcweb_ng_qemu.sh
#
# End-to-end script: cross-compile bmcweb-ng for ARM, boot OpenBMC in QEMU,
# inject the binary, run Redfish smoke tests against bmcweb-ng, then tear down.
#
# Must be run inside WSL2 (Ubuntu) or a native Linux shell.
# On Windows: open Ubuntu WSL and run:
#   bash scripts/run_bmcweb_ng_qemu.sh
#
# Steps performed:
#   1.  Install system prerequisites (Rust, ARM cross-compiler, tools)
#   2.  Cross-compile bmcweb-ng for arm-unknown-linux-gnueabihf
#   3.  Download QEMU binary + OpenBMC qemuarm image (cached after first run)
#   4.  Boot OpenBMC in QEMU with port forwarding
#   5.  Wait for the upstream bmcweb to come up
#   6.  Run smoke tests against upstream bmcweb (baseline)
#   7.  Stop upstream bmcweb, inject bmcweb-ng binary
#   8.  Start bmcweb-ng inside the VM
#   9.  Wait for bmcweb-ng to come up
#  10.  Run the same smoke tests against bmcweb-ng
#  11.  Print combined pass/fail summary
#  12.  Stop QEMU
#
# Environment variables (optional overrides):
#   SKIP_BUILD=1      skip the cargo cross-compile step (use existing binary)
#   SKIP_BASELINE=1   skip the upstream bmcweb smoke tests (step 6)
#   BMC_PORT=2443     host port forwarded to guest HTTPS (default 2443)
#   SSH_PORT=2222     host port forwarded to guest SSH (default 2222)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
WORK_DIR="${REPO_DIR}/target/qemu-test"

ARM_TARGET="arm-unknown-linux-gnueabihf"
BINARY_NAME="bmcwebd-ng"
BINARY_PATH="${REPO_DIR}/target/${ARM_TARGET}/release/${BINARY_NAME}"

BMC_USER="root"
BMC_PASS="0penBmc"
BMC_PORT="${BMC_PORT:-2443}"
SSH_PORT="${SSH_PORT:-2222}"

# ── colours ───────────────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; NC='\033[0m'
info()    { echo -e "${GREEN}[INFO]${NC}  $*"; }
step()    { echo -e "${CYAN}[STEP]${NC}  $*"; }
warn()    { echo -e "${YELLOW}[WARN]${NC}  $*"; }
error()   { echo -e "${RED}[ERROR]${NC} $*" >&2; }
divider() { echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"; }

# ── 0: check we are on Linux/WSL2 ────────────────────────────────────────────
divider
step "0/11  Checking environment"

if [[ "$(uname -s)" != "Linux" ]]; then
    error "This script must run on Linux or WSL2, not $(uname -s)."
    error "On Windows: open Ubuntu WSL and run:  bash scripts/run_bmcweb_ng_qemu.sh"
    exit 1
fi

# ── 1: install prerequisites ──────────────────────────────────────────────────
divider
step "1/11  Installing prerequisites"

apt_install() {
    for pkg in "$@"; do
        if ! dpkg -l "$pkg" &>/dev/null 2>&1; then
            info "Installing $pkg..."
            sudo apt-get install -y "$pkg" >/dev/null 2>&1
        fi
    done
}

sudo apt-get update -qq 2>/dev/null || true

apt_install \
    wget curl jq zstd openssh-client sshpass \
    build-essential pkg-config \
    libpam0g-dev \
    libdbus-1-dev \
    gcc-arm-linux-gnueabihf \
    binutils-arm-linux-gnueabihf

# Install Rust if not present
if ! command -v cargo &>/dev/null; then
    info "Installing Rust via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    # shellcheck disable=SC1090
    source "${HOME}/.cargo/env"
fi

# Ensure cargo is on PATH (rustup installs to ~/.cargo/bin)
export PATH="${HOME}/.cargo/bin:${PATH}"

# Add the ARM target if not already present
if ! rustup target list --installed 2>/dev/null | grep -q "${ARM_TARGET}"; then
    info "Adding Rust target: ${ARM_TARGET}"
    rustup target add "${ARM_TARGET}"
fi

info "Rust: $(rustc --version)"
info "Cargo: $(cargo --version)"
info "ARM cross-compiler: $(arm-linux-gnueabihf-gcc --version | head -1)"

# ── 2: cross-compile bmcweb-ng ────────────────────────────────────────────────
divider
step "2/11  Cross-compiling bmcweb-ng for ${ARM_TARGET}"

if [[ "${SKIP_BUILD:-0}" == "1" ]]; then
    warn "SKIP_BUILD=1: skipping cargo build."
    if [[ ! -f "${BINARY_PATH}" ]]; then
        error "No pre-built binary found at ${BINARY_PATH}. Run without SKIP_BUILD=1 first."
        exit 1
    fi
    info "Using existing binary: ${BINARY_PATH} ($(du -sh "${BINARY_PATH}" | cut -f1))"
else
    info "Building release binary (this may take a few minutes on first run)..."
    cd "${REPO_DIR}"
    cargo build --release --target "${ARM_TARGET}" 2>&1

    if [[ ! -f "${BINARY_PATH}" ]]; then
        error "Build succeeded but binary not found at expected path: ${BINARY_PATH}"
        exit 1
    fi

    local_size=$(du -sh "${BINARY_PATH}" | cut -f1)
    info "Binary built: ${BINARY_PATH} (${local_size})"
    info "ARM ELF check: $(file "${BINARY_PATH}")"
fi

# ── 3–5: run setup_qemu_test.sh to download image + boot + baseline tests ─────
divider
step "3/11  Downloading QEMU binary and OpenBMC image (if not cached)"
step "4/11  Booting OpenBMC in QEMU"
step "5/11  Waiting for OpenBMC to come up"

QEMU_BINARY="${WORK_DIR}/qemu-system-arm"
QEMU_PIDFILE="${WORK_DIR}/qemu.pid"
QEMU_LOG="${WORK_DIR}/qemu.log"
QEMU_IMG_DIR="${WORK_DIR}/image"
KERNEL="${QEMU_IMG_DIR}/uImage"
ROOTFS="${QEMU_IMG_DIR}/obmc-phosphor-image-qemuarm.ext4"
DTB="${QEMU_IMG_DIR}/qemuarm.dtb"

mkdir -p "${WORK_DIR}" "${QEMU_IMG_DIR}"

# ── Download QEMU binary ───────────────────────────────────────────────────────
QEMU_URL="https://jenkins.openbmc.org/job/latest-qemu-x86/lastSuccessfulBuild/artifact/qemu/build/qemu-system-arm"
if [[ ! -f "${QEMU_BINARY}" ]]; then
    info "Downloading QEMU ARM binary from OpenBMC Jenkins CI..."
    wget -q --show-progress -O "${QEMU_BINARY}" "${QEMU_URL}" || {
        error "Failed to download QEMU binary. Check network or see QEMU_SETUP.md."
        exit 1
    }
    chmod +x "${QEMU_BINARY}"
    info "QEMU downloaded: $(du -sh "${QEMU_BINARY}" | cut -f1)"
else
    info "QEMU binary cached: ${QEMU_BINARY}"
fi

if ! "${QEMU_BINARY}" --version 2>/dev/null | grep -q "QEMU"; then
    error "QEMU binary at ${QEMU_BINARY} failed sanity check. Remove it and retry."
    exit 1
fi

# ── Download OpenBMC image ─────────────────────────────────────────────────────
JENKINS_BUILD="https://jenkins.openbmc.org/job/ci-openbmc/job/openbmc/job/main/lastSuccessfulBuild"

download_if_missing() {
    # download_if_missing <dest_file> <url> [<description>]
    local dest="$1" url="$2" desc="${3:-$2}"
    if [[ ! -f "${dest}" ]]; then
        info "Downloading ${desc}..."
        wget -q --show-progress -O "${dest}" "${url}" 2>/dev/null || { rm -f "${dest}"; return 1; }
    fi
}

if [[ ! -f "${KERNEL}" || ! -f "${ROOTFS}" || ! -f "${DTB}" ]]; then
    info "Fetching OpenBMC qemuarm artifact manifest from Jenkins..."
    manifest=$(curl -sf --max-time 30 \
        "${JENKINS_BUILD}/api/json?tree=artifacts[relativePath,fileName]" \
        2>/dev/null || echo "")

    if [[ -n "${manifest}" ]]; then
        kernel_rel=$(echo "${manifest}" | jq -r \
            '.artifacts[].relativePath | select(test("qemuarm.*uImage$"))' 2>/dev/null | head -1)
        rootfs_rel=$(echo "${manifest}" | jq -r \
            '.artifacts[].relativePath | select(test("qemuarm.*ext4\\.zst$"))' 2>/dev/null | head -1)
        dtb_rel=$(echo "${manifest}" | jq -r \
            '.artifacts[].relativePath | select(test("qemuarm.*\\.dtb$"))' 2>/dev/null | head -1)
    fi

    art="${JENKINS_BUILD}/artifact"

    if [[ ! -f "${KERNEL}" ]]; then
        download_if_missing "${KERNEL}" "${art}/${kernel_rel:-openbmc/build/tmp/deploy/images/qemuarm/uImage}" "uImage kernel" \
            || { error "Could not download kernel. See QEMU_SETUP.md."; exit 1; }
    fi

    if [[ ! -f "${ROOTFS}" ]]; then
        zst="${ROOTFS}.zst"
        download_if_missing "${zst}" "${art}/${rootfs_rel:-}" "rootfs (.zst)" || {
            error "Could not download rootfs. See QEMU_SETUP.md."
            exit 1
        }
        info "Decompressing rootfs (may take a minute)..."
        zstd -d "${zst}" -o "${ROOTFS}" --force
        rm -f "${zst}"
    fi

    if [[ ! -f "${DTB}" ]]; then
        download_if_missing "${DTB}" "${art}/${dtb_rel:-openbmc/build/tmp/deploy/images/qemuarm/qemuarm.dtb}" "DTB" \
            || { error "Could not download DTB. See QEMU_SETUP.md."; exit 1; }
    fi

    for f in "${KERNEL}" "${ROOTFS}" "${DTB}"; do
        [[ -f "$f" ]] || { error "Image file still missing: $f"; exit 1; }
    done
    info "All image files present."
else
    info "OpenBMC image already cached in ${QEMU_IMG_DIR}."
fi

# ── Boot QEMU ─────────────────────────────────────────────────────────────────
rw_rootfs="${WORK_DIR}/rootfs-rw.ext4"
cp -f "${ROOTFS}" "${rw_rootfs}"

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
trap 'stop_qemu' EXIT

info "Starting QEMU (OpenBMC qemuarm)..."
"${QEMU_BINARY}" \
    -machine    versatilepb \
    -m          256 \
    -drive      "file=${rw_rootfs},if=virtio,format=raw" \
    -net        "nic" \
    -net        "user,hostfwd=tcp::${BMC_PORT}-:443,hostfwd=tcp::${SSH_PORT}-:22,hostfwd=tcp::2080-:80" \
    -kernel     "${KERNEL}" \
    -dtb        "${DTB}" \
    -append     "root=/dev/vda rw console=ttyAMA0,115200 ignore_loglevel" \
    -display    none \
    -serial     "file:${QEMU_LOG}" \
    -pidfile    "${QEMU_PIDFILE}" \
    -daemonize 2>&1 || {
        error "QEMU failed to start."
        [[ -f "${QEMU_LOG}" ]] && tail -20 "${QEMU_LOG}" 2>/dev/null || true
        exit 1
    }
info "QEMU PID: $(cat "${QEMU_PIDFILE}" 2>/dev/null)"

wait_for_bmc() {
    local label="$1"
    info "Waiting for ${label} to become ready (up to 5 min)..."
    for ((i=0; i<60; i++)); do
        code=$(curl -sk --max-time 5 \
            -u "${BMC_USER}:${BMC_PASS}" \
            -o /dev/null -w "%{http_code}" \
            "https://localhost:${BMC_PORT}/redfish/v1" 2>/dev/null)
        if [[ "${code}" == "200" ]]; then
            info "${label} is up and responding."
            return 0
        fi
        printf "."
        sleep 5
    done
    echo ""
    error "Timeout waiting for ${label}. Boot log: ${QEMU_LOG}"
    return 1
}

wait_for_bmc "upstream bmcweb"

# ── smoke test helpers ────────────────────────────────────────────────────────
# These helpers use three global accumulators:
#   SUITE_PASS  (integer)
#   SUITE_FAIL  (integer)
#   SUITE_RESULTS (array)
# The caller resets them before each suite and copies the results afterwards.

SUITE_PASS=0
SUITE_FAIL=0
SUITE_RESULTS=()

_bmc_get() {
    # Stores response body in BMC_RESPONSE, HTTP code in BMC_HTTP_CODE
    local tmpfile
    tmpfile=$(mktemp)
    BMC_HTTP_CODE=$(curl -sk --max-time 10 \
        -u "${BMC_USER}:${BMC_PASS}" \
        -H "Content-Type: application/json" \
        -o "${tmpfile}" -w "%{http_code}" \
        "https://localhost:${BMC_PORT}$1" 2>/dev/null)
    BMC_RESPONSE=$(cat "${tmpfile}")
    rm -f "${tmpfile}"
}

BMC_RESPONSE=""
BMC_HTTP_CODE=""

check_get() {
    local ep="$1" field="${2:-}" want="${3:-}"
    _bmc_get "${ep}"
    if [[ "${BMC_HTTP_CODE}" != "200" ]]; then
        SUITE_FAIL=$((SUITE_FAIL+1))
        SUITE_RESULTS+=("FAIL  GET ${ep}  [HTTP ${BMC_HTTP_CODE}]")
        return
    fi
    if [[ -n "${field}" ]]; then
        local got
        got=$(echo "${BMC_RESPONSE}" | jq -r "${field}" 2>/dev/null)
        if [[ "${got}" == "${want}" ]]; then
            SUITE_PASS=$((SUITE_PASS+1))
            SUITE_RESULTS+=("PASS  GET ${ep}  (${field}=${want})")
        else
            SUITE_FAIL=$((SUITE_FAIL+1))
            SUITE_RESULTS+=("FAIL  GET ${ep}  (expected ${field}=${want}, got '${got}')")
        fi
    else
        SUITE_PASS=$((SUITE_PASS+1))
        SUITE_RESULTS+=("PASS  GET ${ep}")
    fi
}

check_post() {
    local ep="$1" body="$2" want_code="$3"
    local got_code
    got_code=$(curl -sk --max-time 10 -X POST \
        -H "Content-Type: application/json" -d "${body}" \
        -o /dev/null -w "%{http_code}" \
        "https://localhost:${BMC_PORT}${ep}" 2>/dev/null)
    if [[ "${got_code}" == "${want_code}" ]]; then
        SUITE_PASS=$((SUITE_PASS+1))
        SUITE_RESULTS+=("PASS  POST ${ep}  [HTTP ${got_code}]")
    else
        SUITE_FAIL=$((SUITE_FAIL+1))
        SUITE_RESULTS+=("FAIL  POST ${ep}  (expected ${want_code}, got ${got_code})")
    fi
}

run_redfish_checks() {
    check_get "/redfish/v1" '.RedfishVersion' "1.17.0"
    check_get "/redfish/v1" '."@odata.type"' "#ServiceRoot.v1_15_0.ServiceRoot"
    check_get "/redfish/v1/Systems" '."@odata.type"' "#ComputerSystemCollection.ComputerSystemCollection"
    check_get "/redfish/v1/Systems/system" '.Id' "system"
    check_get "/redfish/v1/Chassis" '."@odata.type"' "#ChassisCollection.ChassisCollection"
    check_get "/redfish/v1/Chassis/chassis" '.Id' "chassis"
    check_get "/redfish/v1/Managers" '."@odata.type"' "#ManagerCollection.ManagerCollection"
    check_get "/redfish/v1/Managers/bmc" '.ManagerType' "BMC"
    check_get "/redfish/v1/SessionService" '.ServiceEnabled' "true"
    check_post "/redfish/v1/SessionService/Sessions" \
        '{"UserName":"root","Password":"0penBmc"}' "201"
    check_get "/redfish/v1/AccountService/Accounts" \
        '."@odata.type"' "#ManagerAccountCollection.ManagerAccountCollection"
    check_get "/redfish/v1/AccountService/Roles/Administrator" '.IsPredefined' "true"
    check_get "/redfish/v1/TaskService" '.ServiceEnabled' "true"
    check_get "/redfish/v1/UpdateService" '.ServiceEnabled' "true"
    check_get "/redfish/v1/UpdateService/FirmwareInventory" \
        '."@odata.type"' "#SoftwareInventoryCollection.SoftwareInventoryCollection"
    check_get "/redfish/v1/EventService" '.ServiceEnabled' "true"
    check_get "/redfish/v1/Managers/bmc/NetworkProtocol" \
        '."@odata.type"' "#ManagerNetworkProtocol.v1_9_0.ManagerNetworkProtocol"
    check_get "/redfish/v1/Managers/bmc/EthernetInterfaces" \
        '."@odata.type"' "#EthernetInterfaceCollection.EthernetInterfaceCollection"
    check_get "/redfish/v1/Chassis/chassis/Power" '."@odata.type"' "#Power.v1_7_2.Power"
    check_get "/redfish/v1/Chassis/chassis/Thermal" '."@odata.type"' "#Thermal.v1_8_0.Thermal"

    # Auth enforcement
    local unauth
    unauth=$(curl -sk --max-time 5 -o /dev/null -w "%{http_code}" \
        "https://localhost:${BMC_PORT}/redfish/v1/Systems" 2>/dev/null)
    if [[ "${unauth}" == "401" ]]; then
        SUITE_PASS=$((SUITE_PASS+1))
        SUITE_RESULTS+=("PASS  Unauthenticated GET /Systems returns 401")
    else
        SUITE_FAIL=$((SUITE_FAIL+1))
        SUITE_RESULTS+=("FAIL  Unauthenticated GET /Systems (expected 401, got ${unauth})")
    fi
}

# ── 6: baseline smoke tests (upstream bmcweb) ─────────────────────────────────
divider
step "6/11  Running smoke tests against upstream bmcweb (baseline)"

BASELINE_PASS=0
BASELINE_FAIL=0
BASELINE_RESULTS=()

if [[ "${SKIP_BASELINE:-0}" != "1" ]]; then
    SUITE_PASS=0; SUITE_FAIL=0; SUITE_RESULTS=()
    run_redfish_checks
    BASELINE_PASS=${SUITE_PASS}
    BASELINE_FAIL=${SUITE_FAIL}
    BASELINE_RESULTS=("${SUITE_RESULTS[@]+"${SUITE_RESULTS[@]}"}")
else
    warn "SKIP_BASELINE=1: skipping upstream bmcweb tests."
fi

# ── 7: stop upstream bmcweb, inject bmcweb-ng ─────────────────────────────────
divider
step "7/11  Stopping upstream bmcweb inside VM"

SSH_OPTS="-o StrictHostKeyChecking=no -o ConnectTimeout=10 -p ${SSH_PORT}"
_ssh() { sshpass -p "${BMC_PASS}" ssh ${SSH_OPTS} "${BMC_USER}@localhost" "$@"; }
_scp() { sshpass -p "${BMC_PASS}" scp ${SSH_OPTS/-p/-P} "$@"; }

_ssh "systemctl stop bmcweb || true"
info "Upstream bmcweb stopped."

divider
step "8/11  Copying bmcweb-ng binary into VM"
_scp "${BINARY_PATH}" "${BMC_USER}@localhost:/usr/bin/${BINARY_NAME}"
_ssh "chmod +x /usr/bin/${BINARY_NAME}"
info "Binary installed: $(_ssh "${BINARY_NAME} --version 2>/dev/null | head -1 || echo 'version unknown'")"

divider
step "9/11  Starting bmcweb-ng inside VM"

# Write a minimal config to the VM
_ssh "mkdir -p /etc/bmcweb /var/lib/bmcweb"
_ssh "cat > /etc/bmcweb/config.toml" <<'EOF'
[server]
bind_address = "0.0.0.0"
port = 443
tls_cert = ""
tls_key = ""
max_connections = 100

[auth]
methods = ["basic", "session"]
session_timeout_seconds = 3600
max_sessions = 64

[features]
redfish = true
dbus_rest = false
kvm = false
virtual_media = false
event_service = true

[logging]
level = "info"
format = "text"

[metrics]
enabled = false
port = 9090
EOF

# Start bmcweb-ng in background, redirect logs to a file
_ssh "RUST_LOG=info nohup /usr/bin/${BINARY_NAME} --config /etc/bmcweb/config.toml \
    > /var/log/bmcweb-ng.log 2>&1 &"
info "bmcweb-ng started."

# Allow a moment for the process to bind the port
sleep 3

wait_for_bmc "bmcweb-ng"

# ── 10: smoke tests against bmcweb-ng ─────────────────────────────────────────
divider
step "10/11  Running smoke tests against bmcweb-ng"

BMCNG_PASS=0
BMCNG_FAIL=0
BMCNG_RESULTS=()
SUITE_PASS=0; SUITE_FAIL=0; SUITE_RESULTS=()
run_redfish_checks
BMCNG_PASS=${SUITE_PASS}
BMCNG_FAIL=${SUITE_FAIL}
BMCNG_RESULTS=("${SUITE_RESULTS[@]+"${SUITE_RESULTS[@]}"}")

# ── 11: print combined results ────────────────────────────────────────────────
divider
step "11/11  Results"

print_suite() {
    local title="$1"
    local -n _p="$2"
    local -n _f="$3"
    local -n _r="$4"

    echo ""
    echo "  ┌── ${title} ──"
    local line
    for line in "${_r[@]}"; do
        if [[ "${line}" == PASS* ]]; then
            echo -e "  │  ${GREEN}${line}${NC}"
        else
            echo -e "  │  ${RED}${line}${NC}"
        fi
    done
    echo "  ├──────────────────────────────────────────────────────"
    echo -e "  │  Total: $((_p+_f))  |  ${GREEN}Pass: ${_p}${NC}  |  ${RED}Fail: ${_f}${NC}"
    echo "  └─────────────────────────────────────────────────────"
}

echo ""
echo "════════════════════════════════════════════════════════"
echo "  Redfish Smoke Test Results — bmcweb-ng QEMU run"
echo "════════════════════════════════════════════════════════"

if [[ "${SKIP_BASELINE:-0}" != "1" ]]; then
    print_suite "Upstream bmcweb (baseline)" BASELINE_PASS BASELINE_FAIL BASELINE_RESULTS
fi
print_suite "bmcweb-ng (Rust rewrite)" BMCNG_PASS BMCNG_FAIL BMCNG_RESULTS

echo ""
echo "════════════════════════════════════════════════════════"
echo ""

# Retrieve bmcweb-ng logs from VM
if [[ "${BMCNG_FAIL}" -gt 0 ]]; then
    warn "Fetching bmcweb-ng log from VM for diagnostics..."
    _ssh "cat /var/log/bmcweb-ng.log" 2>/dev/null | tail -40 || true
fi

TOTAL_FAIL=$((BASELINE_FAIL + BMCNG_FAIL))
if [[ "${TOTAL_FAIL}" -gt 0 ]]; then
    error "Some tests failed (baseline: ${BASELINE_FAIL}, bmcweb-ng: ${BMCNG_FAIL})."
    exit 1
else
    info "All tests passed! bmcweb-ng is API-compatible with upstream bmcweb on QEMU."
fi
