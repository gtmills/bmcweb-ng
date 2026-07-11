#!/usr/bin/env bash
# Build a minimal OpenBMC qemuarm image using the official OpenBMC Docker SDK.
# This takes ~20-40 minutes on first run; subsequent runs use the sstate cache.
#
# The resulting files are placed in:
#   target/qemu-test/image/uImage
#   target/qemu-test/image/obmc-phosphor-image-qemuarm.ext4
#   target/qemu-test/image/qemuarm.dtb
#
# Usage: bash scripts/build_openbmc_image.sh

set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
WORK_DIR="${REPO_DIR}/target/qemu-test"
IMG_DIR="${WORK_DIR}/image"

# Bitbake requires UNIX domain sockets which are NOT supported on Windows NTFS
# mounts (/mnt/c/...). Clone and build must happen on the native Linux filesystem.
# We use ~/openbmc-build (inside WSL's ext4 filesystem) and symlink the output
# images back to WORK_DIR/image when done.
LINUX_BUILD_BASE="${OPENBMC_BUILD_DIR:-${HOME}/openbmc-build}"
OPENBMC_DIR="${LINUX_BUILD_BASE}/openbmc-src"
BITBAKE_BUILD_DIR="${LINUX_BUILD_BASE}/build"

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; NC='\033[0m'
info()  { echo -e "${GREEN}[INFO]${NC}  $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }
step()  { echo -e "${CYAN}[STEP]${NC}  $*"; }

mkdir -p "${IMG_DIR}"

# ── Check prerequisites ────────────────────────────────────────────────────────
step "Checking prerequisites for OpenBMC image build..."

need_cmd() {
    if ! command -v "$1" &>/dev/null; then
        info "Installing $1..."
        sudo apt-get install -y "$2" >/dev/null 2>&1
    fi
}

need_cmd git git
need_cmd python3 python3

mkdir -p "${LINUX_BUILD_BASE}"

# Check disk space on the Linux filesystem (not /mnt/c)
free_gb=$(df -BG "${LINUX_BUILD_BASE}" 2>/dev/null | awk 'NR==2{print $4}' | tr -d 'G')
if [[ "${free_gb:-0}" -lt 30 ]]; then
    warn "Only ${free_gb}GB free on ${LINUX_BUILD_BASE}. OpenBMC build needs ~50GB."
    warn "If your WSL virtual disk is small, expand it or set OPENBMC_BUILD_DIR to a larger path."
fi

# ── Clone OpenBMC (shallow, just the tag) ─────────────────────────────────────
step "Cloning OpenBMC source (tag 2.18.0, shallow)..."

if [[ ! -d "${OPENBMC_DIR}/.git" ]]; then
    info "Cloning into Linux filesystem: ${OPENBMC_DIR}"
    git clone --depth 1 --branch 2.18.0 \
        https://github.com/openbmc/openbmc.git \
        "${OPENBMC_DIR}" 2>&1
    info "Clone complete."
else
    info "OpenBMC source already present at ${OPENBMC_DIR}"
fi

# ── Install Yocto / bitbake dependencies ──────────────────────────────────────
step "Installing Yocto build dependencies..."

sudo apt-get install -y \
    gawk wget git diffstat unzip texinfo gcc build-essential chrpath socat \
    cpio python3 python3-pip python3-pexpect xz-utils debianutils \
    iputils-ping python3-git python3-jinja2 libsdl1.2-dev \
    xterm python3-subunit mesa-common-dev zstd liblz4-tool file locales \
    libacl1 libegl-dev 2>&1 | tail -5

sudo locale-gen en_US.UTF-8 2>/dev/null || true

# ── Configure and build qemuarm ───────────────────────────────────────────────
step "Configuring OpenBMC for qemuarm target..."

cd "${OPENBMC_DIR}"

# The OpenBMC setup script checks ZSH_NAME which is unset in bash under
# 'set -u'. Temporarily disable unbound-variable checking around it.
# Do NOT pipe through | tail — that runs setup in a subshell and loses exports.
set +u
# shellcheck disable=SC1091
. setup qemuarm "${BITBAKE_BUILD_DIR}"
set -u
info "OpenBMC build environment configured. BUILDDIR=${BUILDDIR:-unknown}"

step "Building obmc-phosphor-image for qemuarm (this takes 20-60 min)..."
info "Build dir (Linux fs, supports UNIX sockets): ${BITBAKE_BUILD_DIR}"
info "Monitor progress: tail -f ${BITBAKE_BUILD_DIR}/bitbake.log"

# Set parallel jobs based on CPU count
NCPU=$(nproc)
export BB_NUMBER_THREADS="${NCPU}"
export PARALLEL_MAKE="-j${NCPU}"

# bitbake must run from the build directory
cd "${BITBAKE_BUILD_DIR}"
bitbake obmc-phosphor-image 2>&1 | tee "${BITBAKE_BUILD_DIR}/bitbake.log" | grep -E "^(NOTE|WARNING|ERROR|Build|Running|Setscene)" || true

# ── Copy output images ─────────────────────────────────────────────────────────
step "Copying built images to ${IMG_DIR}..."

DEPLOY="${BITBAKE_BUILD_DIR}/tmp/deploy/images/qemuarm"

if [[ ! -d "${DEPLOY}" ]]; then
    error "Build output not found at ${DEPLOY}"
    error "Check ${WORK_DIR}/openbmc-build/bitbake.log for errors."
    exit 1
fi

# kernel
kernel=$(ls "${DEPLOY}"/uImage 2>/dev/null | head -1)
[[ -n "${kernel}" ]] && cp "${kernel}" "${IMG_DIR}/uImage"

# rootfs
rootfs=$(ls "${DEPLOY}"/obmc-phosphor-image-qemuarm*.ext4 2>/dev/null | head -1)
[[ -n "${rootfs}" ]] && cp "${rootfs}" "${IMG_DIR}/obmc-phosphor-image-qemuarm.ext4"

# dtb
dtb=$(ls "${DEPLOY}"/qemuarm*.dtb 2>/dev/null | head -1)
[[ -n "${dtb}" ]] && cp "${dtb}" "${IMG_DIR}/qemuarm.dtb"

for f in "${IMG_DIR}/uImage" "${IMG_DIR}/obmc-phosphor-image-qemuarm.ext4" "${IMG_DIR}/qemuarm.dtb"; do
    if [[ -f "$f" ]]; then
        info "  $(basename $f): $(du -sh $f | cut -f1)"
    else
        error "Missing: $f"
        exit 1
    fi
done

info "Images ready in ${IMG_DIR}"
info "You can now run: SKIP_BUILD=1 bash scripts/run_bmcweb_ng_qemu.sh"
