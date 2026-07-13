#!/bin/bash
EXT4="$HOME/p10bmc-build/build/tmp/deploy/images/p10bmc/obmc-phosphor-image-p10bmc.ext4"
echo "File: $EXT4"
ls -lh "$EXT4" 2>/dev/null
echo "--- /etc/shadow ---"
debugfs -R "cat /etc/shadow" "$EXT4" 2>/dev/null
echo ""
echo "--- /etc/group (relevant lines) ---"
debugfs -R "cat /etc/group" "$EXT4" 2>/dev/null | grep -E "shellaccess|priv-admin|web|redfish|root"
