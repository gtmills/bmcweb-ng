#!/usr/bin/env python3
"""Reduce BitBake parallelism to avoid OOM on 14GB WSL2."""
import os

fname = os.path.expanduser("~/p10bmc-build/build/conf/local.conf")
content = open(fname).read()
content = content.replace('BB_NUMBER_THREADS = "8"', 'BB_NUMBER_THREADS = "4"')
content = content.replace('PARALLEL_MAKE = "-j8"', 'PARALLEL_MAKE = "-j4"')
open(fname, "w", newline="\n").write(content)
print("Updated local.conf:")
for line in open(fname):
    if "THREAD" in line or "PARALLEL" in line:
        print("  ", line.rstrip())
