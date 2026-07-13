#!/usr/bin/env python3
"""
- Create meta-local/conf/layer.conf
- Inject meta-local into bblayers.conf
"""
import os
import re

base = os.path.expanduser("~/p10bmc-build/ibm-openbmc-src")

# 1. Create layer.conf
layer_conf_dir = os.path.join(base, "meta-local", "conf")
os.makedirs(layer_conf_dir, exist_ok=True)

layer_conf = """\
# Layer config for meta-local — WSL2/dev overrides
BBPATH .= ":${LAYERDIR}"
BBFILES += "${LAYERDIR}/recipes-*/*/*.bb ${LAYERDIR}/recipes-*/*/*.bbappend"
BBFILE_COLLECTIONS += "meta-local"
BBFILE_PATTERN_meta-local = "^${LAYERDIR}/"
BBFILE_PRIORITY_meta-local = "99"
LAYERSERIES_COMPAT_meta-local = "nanbield scarthgap styhead"
"""
with open(os.path.join(layer_conf_dir, "layer.conf"), "w", newline="\n") as f:
    f.write(layer_conf)
print("layer.conf written")

# 2. Add meta-local to bblayers.conf
bblayers_path = os.path.expanduser("~/p10bmc-build/build/conf/bblayers.conf")
with open(bblayers_path) as f:
    content = f.read()

meta_local_path = os.path.join(base, "meta-local")
if meta_local_path in content:
    print("meta-local already in bblayers.conf")
else:
    # Insert before the closing quote
    content = content.rstrip()
    if content.endswith('"'):
        content = content[:-1] + f"  {meta_local_path} \\\n  \"\n"
    else:
        content += f'\n  {meta_local_path} \\\n'
    with open(bblayers_path, "w", newline="\n") as f:
        f.write(content)
    print(f"Added {meta_local_path} to bblayers.conf")

print("--- bblayers.conf ---")
with open(bblayers_path) as f:
    print(f.read())
