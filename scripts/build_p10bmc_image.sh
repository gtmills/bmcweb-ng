#!/usr/bin/env bash
# build_p10bmc_image.sh
#
# Build a minimal p10bmc (IBM Rainier) OpenBMC image using the IBM OpenBMC
# fork and bitbake.  The resulting four image files are placed in:
#
#   target/qemu-test/rainier-image/fitImage-linux.bin
#   target/qemu-test/rainier-image/aspeed-bmc-ibm-rainier.dtb
#   target/qemu-test/rainier-image/obmc-phosphor-initramfs.rootfs.cpio.xz
#   target/qemu-test/rainier-image/obmc-phosphor-image.rootfs.wic.qcow2
#
# Usage:
#   bash scripts/build_p10bmc_image.sh
#   # or via the main script:
#   BUILD_P10BMC=1 bash scripts/run_rainier_qemu.sh
#
# Prerequisites:
#   - WSL2 (Ubuntu 22.04 recommended) or native Linux
#   - ~80 GB free on the Linux filesystem (NOT /mnt/c/ — bitbake needs ext4)
#   - Broadband internet (downloads Yocto layers and toolchain)
#
# The build must run on an ext4/btrfs filesystem.  On WSL2, use the WSL
# virtual disk (e.g. ~/...) and NOT a Windows NTFS mount (/mnt/c/).
# BitBake uses UNIX domain sockets which are not supported on NTFS.
#
# Environment variables:
#   OPENBMC_BUILD_DIR   Override the build root (default ~/p10bmc-build)
#   IBM_OPENBMC_BRANCH  OpenBMC branch to clone   (default master)

set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
WORK_DIR="${REPO_DIR}/target/qemu-test"
IMG_DIR="${WORK_DIR}/rainier-image"

# Build root must live on the native Linux filesystem (ext4) — NOT /mnt/c/.
LINUX_BUILD_BASE="${OPENBMC_BUILD_DIR:-${HOME}/p10bmc-build}"
OPENBMC_DIR="${LINUX_BUILD_BASE}/ibm-openbmc-src"
BITBAKE_BUILD_DIR="${LINUX_BUILD_BASE}/build"

# IBM OpenBMC fork (contains the Rainier machine definition)
IBM_OPENBMC_REPO="https://github.com/ibm-openbmc/openbmc.git"
IBM_OPENBMC_BRANCH="${IBM_OPENBMC_BRANCH:-master}"

# Yocto deploy directory for the rainier machine
DEPLOY_DIR="${BITBAKE_BUILD_DIR}/tmp/deploy/images/rainier"

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; NC='\033[0m'
info()  { echo -e "${GREEN}[INFO]${NC}  $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }
step()  { echo -e "${CYAN}[STEP]${NC}  $*"; }

mkdir -p "${IMG_DIR}" "${LINUX_BUILD_BASE}"

# ── Check we are on Linux ─────────────────────────────────────────────────────
if [[ "$(uname -s)" != "Linux" ]]; then
    error "This script must run on Linux or WSL2."
    exit 1
fi

# ── Check disk space ──────────────────────────────────────────────────────────
step "Checking prerequisites..."

free_gb=$(df -BG "${LINUX_BUILD_BASE}" 2>/dev/null | awk 'NR==2{print $4}' | tr -d 'G')
if [[ "${free_gb:-0}" -lt 50 ]]; then
    warn "Only ${free_gb} GB free on ${LINUX_BUILD_BASE}. p10bmc build needs ~80 GB."
    warn "Continuing anyway — you may run out of space."
fi

# Verify we are NOT on an NTFS/Windows mount (bitbake UNIX socket requirement)
fstype=$(stat -f -c '%T' "${LINUX_BUILD_BASE}" 2>/dev/null || echo "unknown")
if echo "${fstype}" | grep -qi "ntfs\|fat\|cifs"; then
    error "Build directory is on a ${fstype} filesystem."
    error "BitBake requires UNIX domain sockets which are NOT supported on ${fstype}."
    error "Set OPENBMC_BUILD_DIR to a path on the WSL2 ext4 VHD, e.g.:"
    error "  OPENBMC_BUILD_DIR=~/p10bmc-build bash scripts/build_p10bmc_image.sh"
    exit 1
fi

# ── Install Yocto build dependencies ─────────────────────────────────────────
step "Installing Yocto/BitBake build dependencies..."

sudo apt-get update -qq 2>/dev/null || true
sudo apt-get install -y \
    gawk wget git diffstat unzip texinfo gcc build-essential chrpath socat \
    cpio python3 python3-pip python3-pexpect xz-utils debianutils \
    iputils-ping python3-git python3-jinja2 libsdl1.2-dev \
    xterm python3-subunit mesa-common-dev zstd liblz4-tool file locales \
    libacl1 libegl-dev 2>&1 | tail -5

sudo locale-gen en_US.UTF-8 2>/dev/null || true
export LC_ALL=en_US.UTF-8

# ── Clone IBM OpenBMC ─────────────────────────────────────────────────────────
step "Cloning IBM OpenBMC (branch: ${IBM_OPENBMC_BRANCH})..."

if [[ ! -d "${OPENBMC_DIR}/.git" ]]; then
    info "Cloning into: ${OPENBMC_DIR}"
    info "(This is a shallow clone to save time and bandwidth)"
    git clone --depth 1 --branch "${IBM_OPENBMC_BRANCH}" \
        "${IBM_OPENBMC_REPO}" \
        "${OPENBMC_DIR}" 2>&1
    info "Clone complete."
else
    info "IBM OpenBMC source already present at ${OPENBMC_DIR}"
    info "To update: cd ${OPENBMC_DIR} && git pull"
fi

# ── Configure bitbake for the rainier machine ─────────────────────────────────
step "Configuring BitBake for machine=rainier..."

cd "${OPENBMC_DIR}"

# The OpenBMC setup script tests ZSH_NAME which is unset under 'set -u'.
# Temporarily disable the unbound-variable check around sourcing it.
set +u
# shellcheck disable=SC1091
. setup rainier "${BITBAKE_BUILD_DIR}"
set -u
info "BitBake environment sourced. BUILDDIR=${BUILDDIR:-${BITBAKE_BUILD_DIR}}"

# Write a minimal local.conf that:
#   1. Skips the nodejs/webui-vue compile (saves ~40 min, no effect on Redfish)
#   2. Sets a reasonable parallel job count
NCPU=$(nproc)
LOCAL_CONF="${BITBAKE_BUILD_DIR}/conf/local.conf"
if ! grep -q "df-phosphor-no-webui" "${LOCAL_CONF}" 2>/dev/null; then
    info "Adding build optimisations to local.conf..."
    cat >> "${LOCAL_CONF}" <<EOF

# Skip the web UI build — not needed for Redfish testing
DISTROOVERRIDES:append = ":df-phosphor-no-webui"

# Parallel build settings ($(nproc) CPUs detected)
BB_NUMBER_THREADS = "${NCPU}"
PARALLEL_MAKE = "-j${NCPU}"

# Keep the sstate cache so incremental rebuilds are fast
SSTATE_DIR = "${LINUX_BUILD_BASE}/sstate-cache"
DL_DIR     = "${LINUX_BUILD_BASE}/downloads"
EOF
fi

# ── Run bitbake ───────────────────────────────────────────────────────────────
step "Building obmc-phosphor-image for rainier (20–60 min on first run)..."
info "Build dir: ${BITBAKE_BUILD_DIR}"
info "Monitor:   tail -f ${BITBAKE_BUILD_DIR}/bitbake.log"
info "(Subsequent runs reuse the sstate cache and take ~5 min)"

cd "${BITBAKE_BUILD_DIR}"
export BB_NUMBER_THREADS="${NCPU}"
export PARALLEL_MAKE="-j${NCPU}"
bitbake obmc-phosphor-image 2>&1 | \
    tee "${BITBAKE_BUILD_DIR}/bitbake.log" | \
    grep -E "^(NOTE|WARNING|ERROR|Build|Running|Setscene|Summary)" || true

# ── Copy output images ────────────────────────────────────────────────────────
step "Copying built images to ${IMG_DIR}..."

if [[ ! -d "${DEPLOY_DIR}" ]]; then
    error "Yocto deploy directory not found: ${DEPLOY_DIR}"
    error "BitBake may have failed. Check: ${BITBAKE_BUILD_DIR}/bitbake.log"
    exit 1
fi

copy_image() {
    local glob="$1"
    local dest="$2"
    local src
    src=$(ls ${DEPLOY_DIR}/${glob} 2>/dev/null | head -1)
    if [[ -n "${src}" ]]; then
        cp "${src}" "${dest}"
        info "  $(basename "${dest}"): $(du -sh "${dest}" | cut -f1)"
    else
        error "  Image not found matching: ${DEPLOY_DIR}/${glob}"
        return 1
    fi
}

copy_image "fitImage-linux.bin"                              "${IMG_DIR}/fitImage-linux.bin"
copy_image "aspeed-bmc-ibm-rainier.dtb"                     "${IMG_DIR}/aspeed-bmc-ibm-rainier.dtb"
copy_image "obmc-phosphor-initramfs*.rootfs.cpio.xz"        "${IMG_DIR}/obmc-phosphor-initramfs.rootfs.cpio.xz"
copy_image "obmc-phosphor-image*.rootfs.wic.qcow2"          "${IMG_DIR}/obmc-phosphor-image.rootfs.wic.qcow2"

echo ""
info "All images ready in ${IMG_DIR}"
info "Run the test suite with:"
info "  SKIP_BUILD=1 bash scripts/run_rainier_qemu.sh"
info "  # or to also cross-compile bmcweb-ng:"
info "  bash scripts/run_rainier_qemu.sh"
