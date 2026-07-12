#!/usr/bin/env bash
# Launcher: kill old QEMU, clean socket, then run the e2e test
set -e
pkill -f qemu-system-arm 2>/dev/null || true
sleep 1
rm -f /tmp/rainier-serial.sock
exec python3 /mnt/c/Users/GunnarMills/Desktop/ai/downstream-public/bmcweb-ng/scripts/_e2e_test.py
