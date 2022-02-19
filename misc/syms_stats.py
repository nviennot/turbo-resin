#!/usr/bin/env python3
import fileinput
import re

syms = []
for line in fileinput.input():
    line = line.strip()
    m = re.match("^([0-9a-f]+) (.)     (.) (\.[^ ]+)\t([0-9a-f]+) (.+)$", line)
    if m is not None:
        (addr, local, type, section, size, *sym) = m.groups()

        addr = int(addr, base=16)
        size = int(size, base=16)

        syms.append((addr, local, type, section, size, *sym))

syms.sort(key=lambda x: x[4])
for (addr, local, type, section, size, sym) in syms:
    if size != 0:
        print(size, section, sym)
