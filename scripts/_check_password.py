#!/usr/bin/env python3
"""Check what password matches the admin hash in the shadow file."""
import crypt, hashlib

# From /etc/shadow: admin:$6$kkSXteT7FmlZdKMQ$eow...
HASH = "$6$kkSXteT7FmlZdKMQ$GqTb3tXPFx9AJlzTw/8X5RoW2Z.100dT.acuk8AFJfNQYr.ZRL8itMIgLqsdq46RNHgiv78XayOSl.IbR4DFU."

candidates = ["0penBmc", "admin", "root", "p10bmc", "openbmc", "password",
              "0penBmc1", "0penBmc1!", "ibm", "test", "debug",
              "ADMIN", "Admin", "changeme", "default"]

for pw in candidates:
    h = crypt.crypt(pw, HASH)
    if h == HASH:
        print(f"MATCH: password = {repr(pw)}")
        break
    else:
        print(f"  no: {repr(pw)}")
else:
    print("No match found in common passwords")

# Also look at what debug-tweaks does to admin user
import subprocess
r = subprocess.run(
    ["grep", "-r", "admin", "/home/gunnarmills/p10bmc-build/ibm-openbmc-src/meta-phosphor/classes/"],
    capture_output=True, text=True
)
print("\n--- meta-phosphor classes grep for 'admin' ---")
print(r.stdout[:2000])
