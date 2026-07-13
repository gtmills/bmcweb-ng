#!/usr/bin/env python3
"""Write the bbappend file with correct Unix line endings."""
import os

# Key insight: oe_runmake() calls die() on failure which exits the whole
# script (set -e is active). We must use oe_runmake_call() for the first
# pass so we can catch the exit code and retry.
content = (
    "# Fix WSL2 clock-skew: on WSL2 the perl sysroot Config.pm may have a\n"
    "# timestamp slightly in the future, causing MakeMaker to regenerate\n"
    "# the Makefile and exit 1 on the first make pass.\n"
    "# Fix: use oe_runmake_call (no die) for the first pass, retry if needed.\n"
    "do_compile() {\n"
    "    # Normalise perl sysroot timestamps\n"
    '    find "${RECIPE_SYSROOT_NATIVE}" \\( -name "*.pm" -o -name "*.h" \\) -exec touch -m {} + 2>/dev/null || true\n'
    "    # First make pass via oe_runmake_call (does not call die on failure)\n"
    '    if ! oe_runmake_call PASTHRU_INC="${CFLAGS}" LD="${CCLD}"; then\n'
    "        # MakeMaker regenerated the Makefile: retry once\n"
    "        if [ -f Makefile ]; then\n"
    '            bbnote "WSL2 clock-skew: Makefile rebuilt; retrying make..."\n'
    '            oe_runmake PASTHRU_INC="${CFLAGS}" LD="${CCLD}"\n'
    "        else\n"
    '            die "make failed and no Makefile was produced"\n'
    "        fi\n"
    "    fi\n"
    "}\n"
)

dest_dir = os.path.expanduser(
    "~/p10bmc-build/ibm-openbmc-src/meta-local/recipes-extended/perl"
)
os.makedirs(dest_dir, exist_ok=True)
dest = os.path.join(dest_dir, "libxml-namespacesupport-perl_%.bbappend")

with open(dest, "w", newline="\n") as f:
    f.write(content)

print(f"Written {len(content)} bytes")
print("---")
with open(dest) as f:
    print(f.read())
