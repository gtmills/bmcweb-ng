# Fix WSL2 clock-skew: on WSL2 the perl sysroot's Config.pm may have a
# timestamp slightly in the future (relative to the build Makefile), which
# causes MakeMaker to regenerate the Makefile and exit 1 on the first make
# pass ("Please rerun the make command").  We work around this by:
#   1. Touching all sysroot .pm / .h files before compile so they are not
#      newer than the Makefile we just generated in do_configure.
#   2. Running make a second time if the first invocation exits non-zero
#      due to a Makefile-regeneration cycle (MakeMaker pattern).
#
# This is intentionally scoped to the native (build-host) variant only.
do_compile() {
    # Step 1: normalise perl sysroot timestamps so they are not in the future
    find "${RECIPE_SYSROOT_NATIVE}" \( -name "*.pm" -o -name "*.h" \) -exec touch -m {} + 2>/dev/null || true

    # Step 2: first make pass (may regenerate Makefile and exit 1 on WSL2)
    oe_runmake PASTHRU_INC="${CFLAGS}" LD="${CCLD}" || {
        # Check whether a fresh Makefile was produced (MakeMaker retry pattern)
        if [ -f Makefile ]; then
            bbnote "First make pass triggered Makefile regeneration; retrying..."
            oe_runmake PASTHRU_INC="${CFLAGS}" LD="${CCLD}"
        else
            bbfatal "make failed and no Makefile was produced"
        fi
    }
}
