#!/usr/bin/env bash
# Launcher: kill old QEMU, clean socket, then run the e2e test
set -e
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
pkill -f qemu-system-arm 2>/dev/null || true
sleep 1
rm -f /tmp/rainier-serial.sock
exec python3 "${SCRIPT_DIR}/_e2e_test.py"
