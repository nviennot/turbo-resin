#!/usr/bin/env python3
import fileinput

sections = []

for line in fileinput.input():
    line = line.strip()

    # what a mess. But whatever
    (idx, name, size, vma, lma, t) = [None]*6

    try:
        (idx, name, size, vma, lma, t) = line.split()
    except ValueError as e:
        try:
            (idx, name, size, vma, t) = line.split()
            lma = vma
        except ValueError as e:
            continue

    if t == "TEXT" or t == "DATA" or t == "BSS":
        sections.append( (name, int(size, base=16), int(vma, base=16), int(lma, base=16), t) )

sections.sort(key=lambda x: x[2])

def KB(v):
    return v * 1024

regions = [
    {'name': 'FLASH',    'origin': 0x08000000,         'len': KB(256), 'used': 0},
    {'name': 'FLASH_RO', 'origin': 0x08000000+KB(256), 'len': KB(256), 'used': 0},
    {'name': 'RAM',      'origin': 0x20000000,         'len': KB(96),  'used': 0},
]

def hsize(size, rel):
    return f"{size/1024:7.1f}K ({100*size/rel:.1f}%)"

def in_region(region, addr):
    return region['origin'] <= addr < region['origin'] + region['len']

for (name, size, vma, lma, t) in sections:
    # a section can belong in two regions, like .data being both in flash and
    # RAM (non-zero global variables).

    for region in regions:
        if in_region(region, vma) or in_region(region, lma):
            region['used'] += size
            print(f"{region['name']:9} {t:5} {name:14} {hsize(size, region['len'])}")

print()
for region in regions:
    print(f"Total {region['name']:8} {hsize(region['used'], region['len'])}")
