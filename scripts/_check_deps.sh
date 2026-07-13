#!/usr/bin/env bash
# Show dynamic library dependencies of the ARM bmcwebd-ng binary.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
readelf -d "${REPO_ROOT}/target/arm-unknown-linux-gnueabihf/release/bmcwebd-ng" | grep NEEDED
