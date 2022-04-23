#!/usr/bin/env python3

# x/10dx 0x40020000
# x/10dx 0x40020400
# x/10dx 0x40020800
# x/10dx 0x40020c00
# x/10dx 0x40021000
# x/10dx 0x40021400
# x/10dx 0x40021800

config = [
  ('A', 0x6aaa9529, 0x00000000, 0x4fe9d53d, 0x64151541, 0x0000c62e, 0x00008028, 0x00000000, 0x00000000, 0xb0000bb0, 0x000aa771),
  ('B', 0xa9a56a84, 0x00000200, 0x01a55544, 0x55515544, 0x00007fb8, 0x00001380, 0x00000000, 0x00000000, 0x02666000, 0x55507700),
  ('C', 0x00004a08, 0x00000000, 0x00005f0c, 0x00005000, 0x0000ffc5, 0x000000c0, 0x00000000, 0x00000000, 0x00bb00b0, 0x00000000),
  ('D', 0xa56a5a4a, 0x00000000, 0x090050c0, 0x05404000, 0x000078be, 0x00003888, 0x00000000, 0x00000000, 0x00cc00cc, 0xcc000ccc),
  ('E', 0xaaaa8001, 0x00000000, 0x00000001, 0x00000411, 0x0000017f, 0x00000021, 0x00000000, 0x00000000, 0xc0000000, 0xcccccccc),
  ('F', 0x54150000, 0x00000000, 0x54150000, 0x5415aaaa, 0x00005c00, 0x000044a1, 0x00000000, 0x00000000, 0x00000000, 0x00000000),
  ('G', 0x6a954565, 0x00000000, 0x7cd54545, 0x40155545, 0x00009370, 0x00008130, 0x00000000, 0x00000000, 0x00000c00, 0x0bbcb000),
]

for (port, mode, otype, ospeed, pupd, idr, od, bsr, lck, afrl, afrh) in config:
    af = (afrh << 32) | afrl
    for pin in range(16):
        pin_mode = (mode >> (2*pin)) & 0b11
        pin_otype = (otype >> pin) & 1
        pin_ospeed = (ospeed >> (2*pin)) & 0b11
        pin_pupd = (pupd >> (2*pin)) & 0b11
        pin_idr = (idr >> pin) & 1
        pin_od = (od >> pin) & 1
        pin_af = (af >> (4*pin)) & 0b1111

        is_input = pin_mode == 0b00 or pin_mode == 0b11
        desc = []
        if is_input:
            desc.append({
                0b00: 'Input',
                0b11: 'Analog',
            }[pin_mode])

            desc.append({
                0b00: '',
                0b01: ' pull-up',
                0b10: ' pull-down',
                0b11: ' pullup=RESERVED',
            }[pin_pupd])

            desc.append({
                0b0: ' state=0',
                0b1: ' state=1',
            }[pin_idr])
        else:
            desc.append({
                0b01: 'Output',
                0b10: f'Alternate AF{pin_af}',
            }[pin_mode])

            pullup_desc = {
                0b00: '',
                0b01: ' pull-up',
                0b10: ' pull-down',
                0b11: ' pullup=RESERVED',
            }[pin_pupd]

            desc.append({
                0b0: '',
                0b1: f' open-drain{pullup_desc}',
            }[pin_otype])

            desc.append({
                0b00: ' low speed',
                0b01: ' medium speed',
                0b10: ' high speed',
                0b11: ' very-high speed',
            }[pin_ospeed])

            desc.append({
                0b0: ' state=0',
                0b1: ' state=1',
            }[pin_od])

        print(f"P{port}{pin} {''.join(desc)}")
