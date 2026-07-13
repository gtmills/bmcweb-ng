#!/usr/bin/env bash
# run_rainier_qemu.sh
#
# End-to-end script: cross-compile bmcweb-ng for ARM, boot p10bmc (IBM Rainier)
# in QEMU using the rainier-bmc machine model, inject the binary, run Redfish
# smoke tests against bmcweb-ng, then tear down.
#
# Must be run inside WSL2 (Ubuntu) or a native Linux shell.
# On Windows: open Ubuntu WSL and run:
#   bash scripts/run_rainier_qemu.sh
#
# ── IMAGE REQUIREMENT ────────────────────────────────────────────────────────
#
# This script requires a p10bmc image built from the IBM OpenBMC fork
# (ibm-openbmc/openbmc) with machine=rainier.  The generic upstream OpenBMC
# qemuarm image will NOT work — it has the wrong kernel, DTB, and filesystem
# layout for the rainier-bmc QEMU machine model.
#
# First-time setup (builds the image, ~60 min, needs ~80 GB):
#
#   BUILD_P10BMC=1 bash scripts/run_rainier_qemu.sh
#
# Subsequent runs (image cached in target/qemu-test/rainier-image/):
#
#   bash scripts/run_rainier_qemu.sh
#
# If you already have a p10bmc Yocto deploy directory:
#
#   IMAGEPATH=/path/to/tmp/deploy/images/rainier bash scripts/run_rainier_qemu.sh
#
# ── STEPS ────────────────────────────────────────────────────────────────────
#
#   1.  Install system prerequisites (Rust, ARM cross-compiler, tools)
#   2.  Cross-compile bmcweb-ng for arm-unknown-linux-gnueabihf
#   3.  Build p10bmc image from source (if BUILD_P10BMC=1) or verify cached files
#   4.  Boot p10bmc in QEMU (rainier-bmc machine) with port forwarding
#   5.  Wait for upstream bmcweb to come up
#   6.  Run smoke tests against upstream bmcweb (baseline) [SKIP_BASELINE=1 to skip]
#   7.  Stop upstream bmcweb, inject bmcweb-ng binary
#   8.  Write test config and start bmcweb-ng inside the VM
#   9.  Wait for bmcweb-ng to come up
#  10.  Run the same smoke tests against bmcweb-ng
#  11.  Print combined pass/fail summary
#  12.  Stop QEMU
#
# ── ENVIRONMENT VARIABLES ────────────────────────────────────────────────────
#
#   BUILD_P10BMC=1    Build the Yocto p10bmc image from source before running.
#                     Required on first use unless IMAGEPATH is set.
#   IMAGEPATH=<dir>   Directory containing the four pre-built image files.
#   SKIP_BUILD=1      Skip the cargo cross-compile step (use existing binary).
#   SKIP_BASELINE=1   Skip the upstream bmcweb smoke tests (step 6).
#   BMC_PORT=2443     Host port → guest HTTPS 443  (default 2443)
#   SSH_PORT=2222     Host port → guest SSH 22      (default 2222)
#   HTTP_PORT=2080    Host port → guest HTTP 80     (default 2080)
#   BMC_PASS=<pw>     BMC root password             (default 0penBmc)

set -euo pipefail

# Ensure ~/.cargo/bin is on PATH regardless of whether this is an interactive
# shell (wsl -- bash -c "..." does not source ~/.bashrc or ~/.cargo/env).
export PATH="${HOME}/.cargo/bin:${PATH}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
WORK_DIR="${REPO_DIR}/target/qemu-test"
DEFAULT_IMG_DIR="${WORK_DIR}/rainier-image"

# Image directory — can be overridden via IMAGEPATH env var
IMAGEPATH="${IMAGEPATH:-${DEFAULT_IMG_DIR}}"

ARM_TARGET="arm-unknown-linux-gnueabihf"
BINARY_NAME="bmcwebd-ng"
BINARY_PATH="${REPO_DIR}/target/${ARM_TARGET}/release/${BINARY_NAME}"

BMC_USER="root"
BMC_PASS="${BMC_PASS:-0penBmc}"
BMC_PORT="${BMC_PORT:-2443}"
SSH_PORT="${SSH_PORT:-2222}"
HTTP_PORT="${HTTP_PORT:-2080}"

QEMU_PIDFILE="${WORK_DIR}/rainier-qemu.pid"
QEMU_LOG="${WORK_DIR}/rainier-qemu.log"

# ── colours ───────────────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; NC='\033[0m'
info()    { echo -e "${GREEN}[INFO]${NC}  $*"; }
step()    { echo -e "${CYAN}[STEP]${NC}  $*"; }
warn()    { echo -e "${YELLOW}[WARN]${NC}  $*"; }
error()   { echo -e "${RED}[ERROR]${NC} $*" >&2; }
divider() { echo -e "${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"; }

# ── 0: environment check ──────────────────────────────────────────────────────
divider
step "0/11  Checking environment"

if [[ "$(uname -s)" != "Linux" ]]; then
    error "This script must run on Linux or WSL2, not $(uname -s)."
    error "On Windows: open Ubuntu WSL and run:  bash scripts/run_rainier_qemu.sh"
    exit 1
fi

mkdir -p "${WORK_DIR}" "${IMAGEPATH}"

# ── 1: install prerequisites ──────────────────────────────────────────────────
divider
step "1/11  Installing prerequisites"

apt_install() {
    local missing=()
    for pkg in "$@"; do
        if ! dpkg -l "$pkg" &>/dev/null 2>&1; then
            missing+=("$pkg")
        fi
    done
    if [[ "${#missing[@]}" -gt 0 ]]; then
        info "Installing: ${missing[*]}"
        sudo apt-get install -y "${missing[@]}" >/dev/null 2>&1
    fi
}

# Enable universe repo so sshpass is available
if ! grep -rq "^deb.*universe" /etc/apt/sources.list /etc/apt/sources.list.d/ 2>/dev/null; then
    info "Enabling Ubuntu universe repository..."
    sudo add-apt-repository -y universe 2>/dev/null || true
fi
sudo apt-get update -qq 2>/dev/null || true

apt_install \
    wget curl jq zstd openssh-client sshpass \
    build-essential pkg-config \
    libpam0g-dev libdbus-1-dev \
    gcc-arm-linux-gnueabihf binutils-arm-linux-gnueabihf \
    qemu-system-arm libfdt1

# Rainier uses qcow2 images — qemu-img is needed to create a writable overlay
apt_install qemu-utils

# Install Rust if not present
if ! command -v cargo &>/dev/null; then
    info "Installing Rust via rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    # shellcheck disable=SC1090
    source "${HOME}/.cargo/env"
fi
export PATH="${HOME}/.cargo/bin:${PATH}"

# Add the ARM target if not already present
if ! rustup target list --installed 2>/dev/null | grep -q "${ARM_TARGET}"; then
    info "Adding Rust target: ${ARM_TARGET}"
    rustup target add "${ARM_TARGET}"
fi

info "Rust:              $(rustc --version)"
info "Cargo:             $(cargo --version)"
info "ARM cross-compiler: $(arm-linux-gnueabihf-gcc --version | head -1)"
info "QEMU:              $(qemu-system-arm --version 2>/dev/null | head -1)"

# Verify QEMU supports the rainier-bmc machine
if ! qemu-system-arm -machine help 2>/dev/null | grep -q "rainier-bmc"; then
    error "qemu-system-arm does not support the 'rainier-bmc' machine type."
    error "This machine was added in QEMU 7.1.0.  Your installed version:"
    qemu-system-arm --version 2>/dev/null || true
    error "Install a newer QEMU (>=7.1) or use the OpenBMC Jenkins QEMU binary:"
    error "  https://jenkins.openbmc.org/job/latest-qemu-x86/lastSuccessfulBuild/artifact/qemu/build/qemu-system-arm"
    exit 1
fi

# ── 2: cross-compile bmcweb-ng ────────────────────────────────────────────────
divider
step "2/11  Cross-compiling bmcweb-ng for ${ARM_TARGET} (Rainier = AST2600 ARMv7)"

if [[ "${SKIP_BUILD:-0}" == "1" ]]; then
    warn "SKIP_BUILD=1: skipping cargo build."
    if [[ ! -f "${BINARY_PATH}" ]]; then
        error "No pre-built binary found at ${BINARY_PATH}. Run without SKIP_BUILD=1 first."
        exit 1
    fi
    info "Using existing binary: ${BINARY_PATH} ($(du -sh "${BINARY_PATH}" | cut -f1))"
else
    info "Building release binary (PAM disabled for ARM cross-compilation)..."
    cd "${REPO_DIR}"
    # pam feature intentionally omitted — no ARM libpam sysroot needed.
    # In dev/QEMU mode the auth stub accepts any non-empty credentials.
    CC=arm-linux-gnueabihf-gcc \
    CARGO_TARGET_ARM_UNKNOWN_LINUX_GNUEABIHF_LINKER=arm-linux-gnueabihf-gcc \
    cargo build --release --target "${ARM_TARGET}" 2>&1

    if [[ ! -f "${BINARY_PATH}" ]]; then
        error "Build succeeded but binary not found at: ${BINARY_PATH}"
        exit 1
    fi

    info "Binary built: ${BINARY_PATH} ($(du -sh "${BINARY_PATH}" | cut -f1))"
    info "ARM ELF check: $(file "${BINARY_PATH}")"
fi

# ── 3: locate / build p10bmc image ───────────────────────────────────────────
divider
step "3/11  Locating p10bmc Rainier image files"

FIT_IMAGE="${IMAGEPATH}/fitImage-linux.bin"
DTB_FILE="${IMAGEPATH}/aspeed-bmc-ibm-rainier.dtb"
INITRD_FILE="${IMAGEPATH}/obmc-phosphor-initramfs.rootfs.cpio.xz"
WIC_IMAGE="${IMAGEPATH}/obmc-phosphor-image.rootfs.wic.qcow2"

check_images() {
    local ok=1
    for f in "${FIT_IMAGE}" "${DTB_FILE}" "${INITRD_FILE}" "${WIC_IMAGE}"; do
        if [[ -f "$f" ]]; then
            info "  ✓ $(basename "$f") ($(du -sh "$f" | cut -f1))"
        else
            error "  ✗ Missing: $f"
            ok=0
        fi
    done
    [[ "${ok}" -eq 1 ]]
}

if check_images 2>/dev/null; then
    info "All image files present (cached)."
elif [[ "${BUILD_P10BMC:-0}" == "1" ]]; then
    divider
    step "3/11  BUILD_P10BMC=1 — building p10bmc image from source (~60 min)"
    info "Cloning ibm-openbmc/openbmc and running bitbake obmc-phosphor-image machine=rainier"
    bash "${SCRIPT_DIR}/build_p10bmc_image.sh"
    # build_p10bmc_image.sh always writes to ${DEFAULT_IMG_DIR}
    IMAGEPATH="${DEFAULT_IMG_DIR}"
    FIT_IMAGE="${IMAGEPATH}/fitImage-linux.bin"
    DTB_FILE="${IMAGEPATH}/aspeed-bmc-ibm-rainier.dtb"
    INITRD_FILE="${IMAGEPATH}/obmc-phosphor-initramfs.rootfs.cpio.xz"
    WIC_IMAGE="${IMAGEPATH}/obmc-phosphor-image.rootfs.wic.qcow2"
    check_images || { error "Image build finished but expected files are still missing."; exit 1; }
else
    # No cached images and BUILD_P10BMC not set — give a clear, actionable error.
    error ""
    error "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    error "  p10bmc image required but not found"
    error "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    error ""
    error "  The rainier-bmc QEMU machine requires a p10bmc image built"
    error "  from the IBM OpenBMC fork (ibm-openbmc/openbmc, machine=rainier)."
    error "  The generic upstream qemuarm image will NOT work."
    error ""
    error "  TO BUILD THE IMAGE (first time, ~60 min, needs ~80 GB free):"
    error ""
    error "    BUILD_P10BMC=1 bash scripts/run_rainier_qemu.sh"
    error ""
    error "  This clones ibm-openbmc/openbmc and runs:"
    error "    bitbake obmc-phosphor-image  (machine=rainier)"
    error ""
    error "  Subsequent runs reuse the cached image and take only seconds."
    error ""
    error "  IF YOU ALREADY HAVE A p10bmc DEPLOY DIRECTORY:"
    error ""
    error "    IMAGEPATH=/path/to/tmp/deploy/images/rainier \\"
    error "      bash scripts/run_rainier_qemu.sh"
    error ""
    error "  Required files in that directory:"
    error "    fitImage-linux.bin"
    error "    aspeed-bmc-ibm-rainier.dtb"
    error "    obmc-phosphor-initramfs.rootfs.cpio.xz"
    error "    obmc-phosphor-image.rootfs.wic.qcow2"
    error ""
    error "  See QEMU_SETUP.md for full details."
    error ""
    exit 1
fi

# ── 4: boot p10bmc in QEMU ────────────────────────────────────────────────────
divider
step "4/11  Booting p10bmc (rainier-bmc) in QEMU"

# Create a qcow2 overlay on top of the read-only WIC image so the pristine
# image is never modified.  The overlay is discarded on each test run.
RW_WIC="${WORK_DIR}/rainier-rw.qcow2"
info "Creating writable qcow2 overlay over: ${WIC_IMAGE}"
qemu-img create -f qcow2 -b "${WIC_IMAGE}" -F qcow2 "${RW_WIC}" >/dev/null
info "Overlay created: ${RW_WIC}"

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
    # Remove the writable overlay — the base image is untouched
    rm -f "${RW_WIC}"
}
trap 'stop_qemu' EXIT

# Port-forwarding map:
#   Host:${BMC_PORT} → Guest:443   HTTPS (upstream bmcweb / bmcweb-ng)
#   Host:${SSH_PORT} → Guest:22    SSH
#   Host:${HTTP_PORT}→ Guest:80    HTTP (bmcweb-ng plain HTTP fallback)
#
# The rainier-bmc machine command exactly mirrors the upstream IBM p10bmc run
# convention with -daemonize and a pidfile added for process management.
#
# Key flags explained:
#   -M rainier-bmc,boot-emmc=false  — use the SD card (index=2) as boot device,
#                                     not eMMC (which is not populated in QEMU)
#   -kernel fitImage-linux.bin      — FIT image contains kernel + built-in DTB
#   -dtb aspeed-bmc-ibm-rainier.dtb — external DTB overrides the one in the FIT;
#                                     required by the rainier-bmc machine model
#   -initrd ...cpio.xz              — initramfs mounts the squashfs overlay
#   -drive ...,if=sd,index=2        — WIC image presented as SD card slot 2
#   -append "...root=PARTLABEL=rofs-a"  — kernel finds the read-only rootfs
#                                         partition by label inside the WIC

info "Starting QEMU (rainier-bmc)..."
qemu-system-arm \
    -M           "rainier-bmc,boot-emmc=false" \
    -nographic \
    -kernel      "${FIT_IMAGE}" \
    -dtb         "${DTB_FILE}" \
    -initrd      "${INITRD_FILE}" \
    -drive       "file=${RW_WIC},if=sd,index=2,format=qcow2" \
    -netdev      "user,id=net0,hostfwd=tcp::${BMC_PORT}-:443,hostfwd=tcp::${SSH_PORT}-:22,hostfwd=tcp::${HTTP_PORT}-:80" \
    -net         "nic,netdev=net0" \
    -append      "console=ttyS4,115200n8 rootwait root=PARTLABEL=rofs-a" \
    -serial      "file:${QEMU_LOG}" \
    -pidfile     "${QEMU_PIDFILE}" \
    -daemonize   2>&1 || {
        error "QEMU (rainier-bmc) failed to start."
        if [[ -f "${QEMU_LOG}" ]]; then
            echo "--- Last 30 lines of QEMU log ---"
            tail -30 "${QEMU_LOG}" 2>/dev/null || true
            echo "---------------------------------"
        fi
        exit 1
    }

info "QEMU started (PID: $(cat "${QEMU_PIDFILE}" 2>/dev/null || echo 'unknown'))"
info "  HTTPS  → localhost:${BMC_PORT}"
info "  SSH    → localhost:${SSH_PORT}"
info "  HTTP   → localhost:${HTTP_PORT}"
info "  Serial log: ${QEMU_LOG}"
info "  (tail -f ${QEMU_LOG} to watch boot)"

# ── 5: wait for upstream bmcweb ───────────────────────────────────────────────
divider
step "5/11  Waiting for upstream bmcweb to come up (up to 10 min)"

# p10bmc boots slower than qemuarm — the phosphor-state-manager and
# phosphor-user-manager services take longer to initialise on the Rainier
# machine model.  Allow up to 10 minutes (120 × 5s polls).
wait_for_bmc() {
    local label="$1"
    local port="$2"
    local max_retries="${3:-120}"
    info "Waiting for ${label} on port ${port} (up to $((max_retries * 5))s)..."
    local i
    for ((i=0; i<max_retries; i++)); do
        local code
        code=$(curl -sk --max-time 5 \
            -u "${BMC_USER}:${BMC_PASS}" \
            -o /dev/null -w "%{http_code}" \
            "https://localhost:${port}/redfish/v1" 2>/dev/null)
        if [[ "${code}" == "200" ]]; then
            echo ""
            info "${label} is up and responding (HTTP ${code})."
            return 0
        fi
        printf "."
        sleep 5
    done
    echo ""
    error "Timed out waiting for ${label} after $((max_retries * 5))s."
    error "Boot log: ${QEMU_LOG}"
    error "SSH to diagnose: sshpass -p ${BMC_PASS} ssh -p ${SSH_PORT} root@localhost"
    return 1
}

wait_for_bmc "upstream bmcweb (p10bmc)" "${BMC_PORT}" 120

# ── smoke test helpers ────────────────────────────────────────────────────────
SUITE_PASS=0
SUITE_FAIL=0
SUITE_RESULTS=()
BMC_RESPONSE=""
BMC_HTTP_CODE=""

_bmc_get() {
    local tmpfile
    tmpfile=$(mktemp)
    BMC_HTTP_CODE=$(curl -sk --max-time 15 \
        -u "${BMC_USER}:${BMC_PASS}" \
        -H "Content-Type: application/json" \
        -o "${tmpfile}" -w "%{http_code}" \
        "https://localhost:${BMC_PORT}$1" 2>/dev/null)
    BMC_RESPONSE=$(cat "${tmpfile}")
    rm -f "${tmpfile}"
}

_bmc_patch() {
    local endpoint="$1" body="$2"
    curl -sk --max-time 15 -X PATCH \
        -u "${BMC_USER}:${BMC_PASS}" \
        -H "Content-Type: application/json" \
        -d "${body}" \
        -o /dev/null -w "%{http_code}" \
        "https://localhost:${BMC_PORT}${endpoint}" 2>/dev/null
}

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
            SUITE_RESULTS+=("FAIL  GET ${ep}  (want ${field}=${want}, got '${got}')")
        fi
    else
        SUITE_PASS=$((SUITE_PASS+1))
        SUITE_RESULTS+=("PASS  GET ${ep}")
    fi
}

check_get_present() {
    # Passes if the JSON field is present and not null/empty
    local ep="$1" field="$2" label="${3:-$2}"
    _bmc_get "${ep}"
    if [[ "${BMC_HTTP_CODE}" != "200" ]]; then
        SUITE_FAIL=$((SUITE_FAIL+1))
        SUITE_RESULTS+=("FAIL  GET ${ep}  [HTTP ${BMC_HTTP_CODE}]")
        return
    fi
    local got
    got=$(echo "${BMC_RESPONSE}" | jq -r "${field}" 2>/dev/null)
    if [[ -n "${got}" && "${got}" != "null" ]]; then
        SUITE_PASS=$((SUITE_PASS+1))
        SUITE_RESULTS+=("PASS  GET ${ep}  (${label} present: '${got}')")
    else
        SUITE_FAIL=$((SUITE_FAIL+1))
        SUITE_RESULTS+=("FAIL  GET ${ep}  (${label} missing or null)")
    fi
}

check_get_http() {
    local ep="$1" want_code="$2"
    _bmc_get "${ep}"
    if [[ "${BMC_HTTP_CODE}" == "${want_code}" ]]; then
        SUITE_PASS=$((SUITE_PASS+1))
        SUITE_RESULTS+=("PASS  GET ${ep}  [HTTP ${BMC_HTTP_CODE}]")
    else
        SUITE_FAIL=$((SUITE_FAIL+1))
        SUITE_RESULTS+=("FAIL  GET ${ep}  (want HTTP ${want_code}, got ${BMC_HTTP_CODE})")
    fi
}

check_post() {
    local ep="$1" body="$2" want_code="$3"
    local got_code
    got_code=$(curl -sk --max-time 15 -X POST \
        -H "Content-Type: application/json" \
        -d "${body}" \
        -o /dev/null -w "%{http_code}" \
        "https://localhost:${BMC_PORT}${ep}" 2>/dev/null)
    if [[ "${got_code}" == "${want_code}" ]]; then
        SUITE_PASS=$((SUITE_PASS+1))
        SUITE_RESULTS+=("PASS  POST ${ep}  [HTTP ${got_code}]")
    else
        SUITE_FAIL=$((SUITE_FAIL+1))
        SUITE_RESULTS+=("FAIL  POST ${ep}  (want ${want_code}, got ${got_code})")
    fi
}

check_patch() {
    local ep="$1" body="$2" want_code="$3"
    local got_code
    got_code=$(_bmc_patch "${ep}" "${body}")
    if [[ "${got_code}" == "${want_code}" ]]; then
        SUITE_PASS=$((SUITE_PASS+1))
        SUITE_RESULTS+=("PASS  PATCH ${ep}  [HTTP ${got_code}]")
    else
        SUITE_FAIL=$((SUITE_FAIL+1))
        SUITE_RESULTS+=("FAIL  PATCH ${ep}  (want ${want_code}, got ${got_code})")
    fi
}

check_unauth() {
    local ep="$1"
    local got_code
    got_code=$(curl -sk --max-time 5 \
        -o /dev/null -w "%{http_code}" \
        "https://localhost:${BMC_PORT}${ep}" 2>/dev/null)
    if [[ "${got_code}" == "401" ]]; then
        SUITE_PASS=$((SUITE_PASS+1))
        SUITE_RESULTS+=("PASS  Unauthenticated GET ${ep} → 401")
    else
        SUITE_FAIL=$((SUITE_FAIL+1))
        SUITE_RESULTS+=("FAIL  Unauthenticated GET ${ep} (want 401, got ${got_code})")
    fi
}

run_redfish_checks() {
    # ServiceRoot
    check_get "/redfish/v1" '.RedfishVersion' "1.17.0"
    check_get "/redfish/v1" '."@odata.type"' "#ServiceRoot.v1_15_0.ServiceRoot"

    # Systems
    check_get "/redfish/v1/Systems" \
        '."@odata.type"' "#ComputerSystemCollection.ComputerSystemCollection"
    check_get "/redfish/v1/Systems/system" '.Id' "system"
    check_get "/redfish/v1/Systems/system" '."@odata.type"' \
        "#ComputerSystem.v1_20_0.ComputerSystem"
    check_get_present "/redfish/v1/Systems/system" '.PowerState' "PowerState"
    check_get_present "/redfish/v1/Systems/system" '.AssetTag' "AssetTag"
    check_get_present "/redfish/v1/Systems/system" \
        '.Boot.BootSourceOverrideTarget' "BootSourceOverrideTarget"

    # Chassis
    check_get "/redfish/v1/Chassis" \
        '."@odata.type"' "#ChassisCollection.ChassisCollection"
    check_get "/redfish/v1/Chassis/chassis" '.Id' "chassis"
    check_get_present "/redfish/v1/Chassis/chassis" '.IndicatorLED' "IndicatorLED"
    check_get "/redfish/v1/Chassis/chassis/Power" \
        '."@odata.type"' "#Power.v1_7_2.Power"
    check_get "/redfish/v1/Chassis/chassis/Thermal" \
        '."@odata.type"' "#Thermal.v1_8_0.Thermal"
    check_get "/redfish/v1/Chassis/chassis/NetworkAdapters" \
        '."@odata.type"' "#NetworkAdapterCollection.NetworkAdapterCollection"

    # Managers
    check_get "/redfish/v1/Managers" \
        '."@odata.type"' "#ManagerCollection.ManagerCollection"
    check_get "/redfish/v1/Managers/bmc" '.ManagerType' "BMC"
    check_get_present "/redfish/v1/Managers/bmc" '.FirmwareVersion' "FirmwareVersion"
    check_get "/redfish/v1/Managers/bmc/NetworkProtocol" \
        '."@odata.type"' "#ManagerNetworkProtocol.v1_9_0.ManagerNetworkProtocol"
    check_get_present "/redfish/v1/Managers/bmc/NetworkProtocol" '.HostName' "HostName"
    check_get "/redfish/v1/Managers/bmc/EthernetInterfaces" \
        '."@odata.type"' "#EthernetInterfaceCollection.EthernetInterfaceCollection"
    check_get_present "/redfish/v1/Managers/bmc/EthernetInterfaces/eth0" \
        '.MACAddress' "MACAddress"
    check_get "/redfish/v1/Managers/bmc/LogServices/BMC" '.Id' "BMC"
    check_get "/redfish/v1/Managers/bmc/LogServices/BMC/Entries" \
        '."@odata.type"' "#LogEntryCollection.LogEntryCollection"

    # SessionService
    check_get "/redfish/v1/SessionService" '.ServiceEnabled' "true"
    check_post "/redfish/v1/SessionService/Sessions" \
        '{"UserName":"root","Password":"'"${BMC_PASS}"'"}' "201"

    # AccountService
    check_get "/redfish/v1/AccountService" \
        '."@odata.type"' "#AccountService.v1_12_0.AccountService"
    check_get "/redfish/v1/AccountService/Accounts" \
        '."@odata.type"' "#ManagerAccountCollection.ManagerAccountCollection"
    check_get "/redfish/v1/AccountService/Roles/Administrator" '.IsPredefined' "true"
    check_get "/redfish/v1/AccountService/PrivilegeMap" '.Id' "PrivilegeMap"

    # Services
    check_get "/redfish/v1/TaskService" '.ServiceEnabled' "true"
    check_get "/redfish/v1/UpdateService" '.ServiceEnabled' "true"
    check_get "/redfish/v1/UpdateService/FirmwareInventory" \
        '."@odata.type"' "#SoftwareInventoryCollection.SoftwareInventoryCollection"
    check_get "/redfish/v1/EventService" '.ServiceEnabled' "true"

    # Systems sub-resources
    check_get "/redfish/v1/Systems/system/Processors" \
        '."@odata.type"' "#ProcessorCollection.ProcessorCollection"
    check_get "/redfish/v1/Systems/system/Memory" \
        '."@odata.type"' "#MemoryCollection.MemoryCollection"
    check_get "/redfish/v1/Systems/system/LogServices/EventLog" '.Id' "EventLog"
    check_get_present "/redfish/v1/Systems/system/LogServices/EventLog" \
        '.Entries."@odata.id"' "Entries link"
    check_get "/redfish/v1/Systems/system/LogServices/EventLog/Entries" \
        '."@odata.type"' "#LogEntryCollection.LogEntryCollection"

    # Registries / Schemas
    check_get "/redfish/v1/Registries" \
        '."@odata.type"' "#MessageRegistryFileCollection.MessageRegistryFileCollection"
    check_get "/redfish/v1/JsonSchemas" \
        '."@odata.type"' "#JsonSchemaFileCollection.JsonSchemaFileCollection"

    # CertificateService / TelemetryService
    check_get "/redfish/v1/CertificateService" '.Id' "CertificateService"
    check_get "/redfish/v1/TelemetryService" '.Id' "TelemetryService"
    check_get "/redfish/v1/TelemetryService/MetricDefinitions" \
        '."@odata.type"' "#MetricDefinitionCollection.MetricDefinitionCollection"

    # Auth enforcement — unauthenticated GET must return 401
    check_unauth "/redfish/v1/Systems"

    # PATCH operations
    check_patch "/redfish/v1/Systems/system" \
        '{"Boot":{"BootSourceOverrideTarget":"Pxe","BootSourceOverrideEnabled":"Once"}}' "200"
    check_patch "/redfish/v1/SessionService" \
        '{"SessionTimeout":1800}' "200"
    check_patch "/redfish/v1/EventService" \
        '{"DeliveryRetryAttempts":5}' "200"

    # Verify PATCH values were persisted
    check_get "/redfish/v1/SessionService" '.SessionTimeout' "1800"
    check_get "/redfish/v1/EventService" '.DeliveryRetryAttempts' "5"

    # Health endpoint (bmcweb-ng only — upstream bmcweb does not have /health)
    # Skipped in baseline run via the RUNNING_BASELINE flag.
    if [[ "${RUNNING_BASELINE:-0}" != "1" ]]; then
        _bmc_get "/health"
        if [[ "${BMC_HTTP_CODE}" == "200" ]]; then
            local ver
            ver=$(echo "${BMC_RESPONSE}" | jq -r '.version' 2>/dev/null)
            if [[ -n "${ver}" && "${ver}" != "null" ]]; then
                SUITE_PASS=$((SUITE_PASS+1))
                SUITE_RESULTS+=("PASS  GET /health  (version=${ver})")
            else
                SUITE_FAIL=$((SUITE_FAIL+1))
                SUITE_RESULTS+=("FAIL  GET /health  (version field missing)")
            fi
        else
            SUITE_FAIL=$((SUITE_FAIL+1))
            SUITE_RESULTS+=("FAIL  GET /health  [HTTP ${BMC_HTTP_CODE}]")
        fi
    fi
}

# ── 6: baseline smoke tests against upstream bmcweb ──────────────────────────
divider
step "6/11  Running smoke tests against upstream bmcweb (p10bmc baseline)"

BASELINE_PASS=0; BASELINE_FAIL=0; BASELINE_RESULTS=()

if [[ "${SKIP_BASELINE:-0}" != "1" ]]; then
    RUNNING_BASELINE=1
    SUITE_PASS=0; SUITE_FAIL=0; SUITE_RESULTS=()
    run_redfish_checks
    RUNNING_BASELINE=0
    BASELINE_PASS=${SUITE_PASS}
    BASELINE_FAIL=${SUITE_FAIL}
    BASELINE_RESULTS=("${SUITE_RESULTS[@]+"${SUITE_RESULTS[@]}"}")
    info "Baseline: ${BASELINE_PASS} passed, ${BASELINE_FAIL} failed."
else
    warn "SKIP_BASELINE=1: skipping upstream bmcweb tests."
fi

# ── 7: stop upstream bmcweb ───────────────────────────────────────────────────
divider
step "7/11  Stopping upstream bmcweb inside VM"

SSH_OPTS="-o StrictHostKeyChecking=no -o ConnectTimeout=15 -p ${SSH_PORT}"
_ssh() { sshpass -p "${BMC_PASS}" ssh ${SSH_OPTS} "${BMC_USER}@localhost" "$@"; }
_scp() { sshpass -p "${BMC_PASS}" scp -O -P "${SSH_PORT}" \
             -o StrictHostKeyChecking=no "$@"; }

_ssh "systemctl stop bmcweb.socket bmcweb.service 2>/dev/null || \
      systemctl stop bmcweb 2>/dev/null || true"
info "Upstream bmcweb stopped."

# ── 8: copy bmcweb-ng binary into VM ─────────────────────────────────────────
divider
step "8/11  Copying bmcweb-ng binary into VM (/tmp — tmpfs avoids rootfs space)"

# p10bmc rofs-a is a read-only squashfs; /tmp is a tmpfs with plenty of space.
_scp "${BINARY_PATH}" "${BMC_USER}@localhost:/tmp/${BINARY_NAME}"
_ssh "chmod +x /tmp/${BINARY_NAME}"
info "Binary installed at /tmp/${BINARY_NAME}"
info "Version: $(_ssh "RUST_LOG=error /tmp/${BINARY_NAME} --version 2>/dev/null || echo '(version flag not available)'")"

# ── 9: write config and start bmcweb-ng ──────────────────────────────────────
divider
step "9/11  Starting bmcweb-ng inside VM (plain HTTP on port 443)"

_ssh "mkdir -p /tmp/bmcweb-ng-config /var/lib/bmcweb 2>/dev/null || true"

# Write a minimal test config.
# TLS is disabled (tls_cert = "") so bmcweb-ng falls through to run_plain_http().
# The test suite still connects via https://localhost:${BMC_PORT} because the
# upstream bmcweb TLS port is reused — both services cannot run simultaneously.
# bmcweb-ng binds :443 plain HTTP; curl -sk treats it as HTTPS but the
# handshake will fail.  We therefore have bmcweb-ng bind :80 (HTTP_PORT) and
# adjust the test helpers below to use that port.
_ssh "cat > /tmp/bmcweb-ng-config/config.toml" <<'TOML_EOF'
[server]
bind_address = "0.0.0.0"
port = 80
tls_cert = ""
tls_key  = ""
max_connections = 100

[auth]
session_timeout_seconds = 3600
max_sessions = 64

[logging]
level = "info"

[metrics]
enabled = false
port = 9090
TOML_EOF

_ssh "RUST_LOG=info nohup /tmp/${BINARY_NAME} \
    --config /tmp/bmcweb-ng-config/config.toml \
    > /tmp/bmcweb-ng.log 2>&1 &"
info "bmcweb-ng started (plain HTTP on :80, forwarded to host :${HTTP_PORT})."

# Give the process a moment to bind the port
sleep 3

# Wait for bmcweb-ng — it answers plain HTTP on port 80 (host: HTTP_PORT)
wait_for_bmc_http() {
    local label="$1"
    local port="$2"
    local max_retries="${3:-30}"
    info "Waiting for ${label} on HTTP port ${port}..."
    local i
    for ((i=0; i<max_retries; i++)); do
        local code
        code=$(curl -s --max-time 5 \
            -u "${BMC_USER}:${BMC_PASS}" \
            -o /dev/null -w "%{http_code}" \
            "http://localhost:${port}/redfish/v1" 2>/dev/null)
        if [[ "${code}" == "200" ]]; then
            echo ""
            info "${label} is up (HTTP ${code})."
            return 0
        fi
        printf "."
        sleep 2
    done
    echo ""
    error "Timed out waiting for ${label}."
    _ssh "cat /tmp/bmcweb-ng.log" 2>/dev/null | tail -30 || true
    return 1
}

wait_for_bmc_http "bmcweb-ng" "${HTTP_PORT}" 30

# ── 10: smoke tests against bmcweb-ng ────────────────────────────────────────
divider
step "10/11  Running smoke tests against bmcweb-ng (plain HTTP :${HTTP_PORT})"

# Override helpers to use plain HTTP against HTTP_PORT
_bmc_get() {
    local tmpfile
    tmpfile=$(mktemp)
    BMC_HTTP_CODE=$(curl -s --max-time 15 \
        -u "${BMC_USER}:${BMC_PASS}" \
        -H "Content-Type: application/json" \
        -o "${tmpfile}" -w "%{http_code}" \
        "http://localhost:${HTTP_PORT}$1" 2>/dev/null)
    BMC_RESPONSE=$(cat "${tmpfile}")
    rm -f "${tmpfile}"
}
_bmc_patch() {
    curl -s --max-time 15 -X PATCH \
        -u "${BMC_USER}:${BMC_PASS}" \
        -H "Content-Type: application/json" \
        -d "$2" \
        -o /dev/null -w "%{http_code}" \
        "http://localhost:${HTTP_PORT}$1" 2>/dev/null
}
check_post() {
    local ep="$1" body="$2" want_code="$3"
    local got_code
    got_code=$(curl -s --max-time 15 -X POST \
        -H "Content-Type: application/json" \
        -d "${body}" \
        -o /dev/null -w "%{http_code}" \
        "http://localhost:${HTTP_PORT}${ep}" 2>/dev/null)
    if [[ "${got_code}" == "${want_code}" ]]; then
        SUITE_PASS=$((SUITE_PASS+1))
        SUITE_RESULTS+=("PASS  POST ${ep}  [HTTP ${got_code}]")
    else
        SUITE_FAIL=$((SUITE_FAIL+1))
        SUITE_RESULTS+=("FAIL  POST ${ep}  (want ${want_code}, got ${got_code})")
    fi
}
check_unauth() {
    local ep="$1"
    local got_code
    got_code=$(curl -s --max-time 5 \
        -o /dev/null -w "%{http_code}" \
        "http://localhost:${HTTP_PORT}${ep}" 2>/dev/null)
    if [[ "${got_code}" == "401" ]]; then
        SUITE_PASS=$((SUITE_PASS+1))
        SUITE_RESULTS+=("PASS  Unauthenticated GET ${ep} → 401")
    else
        SUITE_FAIL=$((SUITE_FAIL+1))
        SUITE_RESULTS+=("FAIL  Unauthenticated GET ${ep} (want 401, got ${got_code})")
    fi
}

BMCNG_PASS=0; BMCNG_FAIL=0; BMCNG_RESULTS=()
RUNNING_BASELINE=0
SUITE_PASS=0; SUITE_FAIL=0; SUITE_RESULTS=()
run_redfish_checks
BMCNG_PASS=${SUITE_PASS}
BMCNG_FAIL=${SUITE_FAIL}
BMCNG_RESULTS=("${SUITE_RESULTS[@]+"${SUITE_RESULTS[@]}"}")

# ── 11: results ───────────────────────────────────────────────────────────────
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
    for line in "${_r[@]+"${_r[@]}"}"; do
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
echo "  Redfish Smoke Test Results — bmcweb-ng on p10bmc/Rainier"
echo "════════════════════════════════════════════════════════"

if [[ "${SKIP_BASELINE:-0}" != "1" ]]; then
    print_suite "Upstream bmcweb — p10bmc baseline (HTTPS :${BMC_PORT})" \
        BASELINE_PASS BASELINE_FAIL BASELINE_RESULTS
fi
print_suite "bmcweb-ng Rust rewrite (HTTP :${HTTP_PORT})" \
    BMCNG_PASS BMCNG_FAIL BMCNG_RESULTS

echo ""
echo "════════════════════════════════════════════════════════"
echo ""

if [[ "${BMCNG_FAIL}" -gt 0 ]]; then
    warn "Fetching bmcweb-ng log from VM for diagnostics..."
    _ssh "cat /tmp/bmcweb-ng.log" 2>/dev/null | tail -50 || true
fi

TOTAL_FAIL=$((${BASELINE_FAIL:-0} + BMCNG_FAIL))
if [[ "${TOTAL_FAIL}" -gt 0 ]]; then
    error "Some tests failed (baseline: ${BASELINE_FAIL:-0}, bmcweb-ng: ${BMCNG_FAIL})."
    exit 1
else
    info "All tests passed! bmcweb-ng is API-compatible with p10bmc upstream bmcweb on Rainier QEMU."
fi
