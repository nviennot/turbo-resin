#!/usr/bin/env python3

import fileinput
import re
import sys


expected_addr = 0
for line in fileinput.input():
    line = line.strip()


    m = re.match("^([0-9a-f]{8}) \[([0-9a-z, ]+)\]$", line)
    if m is not None:
        addr, values = m.groups()
        addr = int(addr, 16)
        values = [int(v, 16) for v in values.split(', ')]

        if addr != expected_addr:
            raise RuntimeError(f"Expected address {hex(expected_addr)} but had {hex(addr)}")

        expected_addr += 16
        sys.stdout.buffer.write(bytes(values))
