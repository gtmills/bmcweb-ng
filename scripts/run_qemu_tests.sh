#!/usr/bin/env bash
# run_qemu_tests.sh
#
# Quick entrypoint — wraps setup_qemu_test.sh but first verifies that
# WSL2 / Linux is available if called from Windows.
#
# Usage (from Windows PowerShell):
#   wsl bash scripts/run_qemu_tests.sh
#
# Usage (from Linux / WSL2):
#   bash scripts/run_qemu_tests.sh

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec bash "${SCRIPT_DIR}/setup_qemu_test.sh" "$@"
