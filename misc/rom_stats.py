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

ROM_SIZE = 256*1024
RAM_SIZE = 96*1024
RAM_ORIGIN = 0x20000000

total_rom_size = 0
total_text_size = 0
total_data_size = 0
total_bss_size = 0
total_ram_size = 0;

def hsize(size, rel):
    return f"{size/1024:7.1f}K ({100*size/rel:.1f}%)"

for (name, size, vma, lma, t) in sections:
    if t == "TEXT":
        total_text_size += size
        total_rom_size += size
        print(f"{name:14} {t:4} {hsize(size, ROM_SIZE)}")
    elif t == "DATA":
        total_data_size += size
        total_rom_size += size
        if vma >= RAM_ORIGIN:
            total_ram_size += size
            print(f"{name:14} {t:4} {hsize(size, RAM_SIZE)}")
        else:
            print(f"{name:14} {t:4} {hsize(size, ROM_SIZE)}")
    elif t == "BSS":
        total_bss_size += size
        total_ram_size += size
        print(f"{name:14} {t:4} {hsize(size, RAM_SIZE)}")

#print()
#print(f"{'Total text':14} {hsize(total_text_size, ROM_SIZE)}")
#print(f"{'Total data':14} {hsize(total_data_size, RAM_SIZE)}")
#print(f"{'Total  bss':14} {hsize(total_bss_size, RAM_SIZE)}")
print()
print(f"{'Total ROM':14} {hsize(total_rom_size, ROM_SIZE)}")
print(f"{'Total RAM':14} {hsize(total_ram_size, RAM_SIZE)}")
