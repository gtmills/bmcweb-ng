#!/usr/bin/env bash
# Internal helper: run the actual p10bmc bitbake build.
# Disables FIT/SPL signing (requires no signing keys) for a QEMU-only build.
# Uses machine=p10bmc (the Yocto name) which produces rainier-bmc QEMU images.
set -euo pipefail
export LC_ALL=en_US.UTF-8
export PATH="$HOME/.cargo/bin:$PATH"

OPENBMC_DIR="$HOME/p10bmc-build/ibm-openbmc-src"
BUILD_DIR="$HOME/p10bmc-build/build"
NCPU=$(nproc)

echo "==> Sourcing OpenBMC setup for machine=p10bmc (NCPU=${NCPU})..."
cd "${OPENBMC_DIR}"
set +u
# shellcheck disable=SC1091
. setup p10bmc "${BUILD_DIR}"
set -u
echo "==> BUILDDIR=${BUILD_DIR}"

LOCAL_CONF="${BUILD_DIR}/conf/local.conf"
if ! grep -q 'bmcweb-ng-qemu-overrides' "${LOCAL_CONF}" 2>/dev/null; then
    echo "==> Writing local.conf overrides..."
    cat >> "${LOCAL_CONF}" <<'EOF'

# ── bmcweb-ng QEMU build overrides ───────────────────────────────────────────
# Marker so this block is only appended once
# bmcweb-ng-qemu-overrides

# Skip web UI build — not needed for Redfish testing (saves ~40 min)
DISTROOVERRIDES:append = ":df-phosphor-no-webui"

# Disable FIT image signing and SPL secure boot — these require
# OEM signing keys that are not available in a development build environment.
# QEMU boots using a plain zImage directly (bypasses u-boot entirely),
# so signed FIT images are not needed for QEMU testing.
UBOOT_SIGN_ENABLE = "0"
SPL_SIGN_ENABLE = "0"
UBOOT_FITIMAGE_ENABLE = "0"
SOCSEC_SIGN_ENABLE = "0"

# Persistent caches — keep sstate and downloads between runs
SSTATE_DIR = "${HOME}/p10bmc-build/sstate-cache"
DL_DIR     = "${HOME}/p10bmc-build/downloads"
EOF

    # Write parallel settings with variable expansion (separate heredoc)
    cat >> "${LOCAL_CONF}" <<VAREOF

# Parallel build settings (${NCPU} CPUs)
BB_NUMBER_THREADS = "${NCPU}"
PARALLEL_MAKE = "-j${NCPU}"
VAREOF
    echo "==> local.conf written"
fi

echo "==> Starting bitbake obmc-phosphor-image (machine=p10bmc)"
echo "    First run: ~60 min. Watch: tail -f ${BUILD_DIR}/bitbake.log"
cd "${BUILD_DIR}"
bitbake obmc-phosphor-image 2>&1 | \
    tee "${BUILD_DIR}/bitbake.log" | \
    grep --line-buffered -E '^(NOTE|WARNING|ERROR|Build|Running|Setscene|Summary|Currently|Tasks)' || true

echo "==> bitbake finished"
