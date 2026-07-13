#!/usr/bin/env bash
pkill -f qemu-system-arm 2>/dev/null; sleep 1
rm -f /tmp/test-serial.sock /tmp/qemu-boot.log
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IMGDIR="${SCRIPT_DIR}/../target/qemu-test/rainier-image"
# Launch QEMU, redirect stdout to a dedicated boot log file
# With -nographic, serial0 goes to stdio (stdout).
# Our -serial null flags are serial1-4, and the socket is serial5.
# The kernel console= is ttyS4, which is serial4 → our 5th -serial = null!
# So we actually need 4 nulls BEFORE the socket (for serial0-3, where serial0
# is taken by -nographic → stdio) -- wait, no: -nographic sets serial0→stdio
# automatically ONLY IF we don't provide -serial flags.
# With explicit -serial flags, the first -serial flag IS serial0.
# So: serial0=null, serial1=null, serial2=null, serial3=null, serial4=socket
# But -nographic redirects serial0→stdio too if provided...
# Let's test: use -serial file:/tmp/ttyS0.txt for serial0 to see what we get
qemu-system-arm -M rainier-bmc -nographic \
  -kernel "$IMGDIR/zImage" \
  -dtb "$IMGDIR/aspeed-bmc-ibm-rainier.dtb" \
  -initrd "$IMGDIR/obmc-phosphor-initramfs.rootfs.cpio.xz" \
  -drive file="$IMGDIR/obmc-phosphor-image.rootfs.wic.qcow2",if=sd,index=2,snapshot=on \
  -append 'console=ttyS4,115200n8 rootwait root=PARTLABEL=rofs-a systemd.watchdog-device= aspeed_wdt.nowdt=1' \
  -serial file:/tmp/ttyS0.txt \
  -serial file:/tmp/ttyS1.txt \
  -serial file:/tmp/ttyS2.txt \
  -serial file:/tmp/ttyS3.txt \
  -serial unix:/tmp/test-serial.sock,server,nowait \
  -net nic -net 'user,hostfwd=tcp::2443-:443,hostfwd=tcp::2080-:80,hostfwd=tcp::2222-:22' \
  >/tmp/qemu-boot.log 2>&1 &
QPID=$!
echo "QEMU PID=$QPID"
echo "Waiting 90s..."
sleep 90
echo "=== qemu-boot.log ($( wc -c < /tmp/qemu-boot.log) bytes): ==="
cat /tmp/qemu-boot.log | head -20
echo "=== ttyS0 ($( wc -c < /tmp/ttyS0.txt 2>/dev/null || echo 0) bytes): ===" 
head -5 /tmp/ttyS0.txt 2>/dev/null || echo "(empty)"
echo "=== ttyS1 ($( wc -c < /tmp/ttyS1.txt 2>/dev/null || echo 0) bytes): ===" 
head -5 /tmp/ttyS1.txt 2>/dev/null || echo "(empty)"
echo "=== ttyS2 ($( wc -c < /tmp/ttyS2.txt 2>/dev/null || echo 0) bytes): ===" 
head -5 /tmp/ttyS2.txt 2>/dev/null || echo "(empty)"
echo "=== ttyS3 ($( wc -c < /tmp/ttyS3.txt 2>/dev/null || echo 0) bytes): ===" 
head -5 /tmp/ttyS3.txt 2>/dev/null || echo "(empty)"
echo "=== socket data (10s): ==="
timeout 10 nc -U /tmp/test-serial.sock 2>/dev/null | cat | head -20
kill $QPID 2>/dev/null
