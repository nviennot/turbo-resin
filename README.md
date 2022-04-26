[![Discord](https://img.shields.io/discord/940395991016828980?label=Discord&logo=discord&logoColor=white)](https://discord.gg/9HSMNYxPAM)

Turbo Resin: open-source firmware for resin printers
====================================================

![Turbo Resin](misc/turbo_resin.jpg)

Turbo Resin is an open-source firmware for SLA resin printers.

This is the implementation of a firmware based on the
[Reverse engineering of the Anycubic Photon Mono 4K](https://github.com/nviennot/reversing-mono4k#readme)

## Roadmap

Drivers:
* [X] Read the touch screen
* [X] Display on the touch screen
* [X] Use the LVGL UI library
* [X] Control the stepper motor
* [X] Read/write to external Flash
* [ ] Read/write to external EEPROM
* [X] Drive the LCD panel
* [X] Read from USB flash drive
* [ ] Control the UV light
* [X] Z=0 detection
* [ ] Being able to flash firmware via USB

Printing features:
* [X] Better Z-Axis motion control for faster prints with fast deceleration
* [X] Z=0 calibration
* [X] Read .pwma files from USB flash drive
* [ ] Support multiple exposure (like RERF but configurable)
* [ ] Print algorithm
* [ ] Support more file formats to support various slicers
* [ ] Add support for other printers
* [ ] Build-plate force feedback for speed optimization. Following [the work of Jan Mrázek](https://blog.honzamrazek.cz/2022/01/prints-not-sticking-to-the-build-plate-layer-separation-rough-surface-on-a-resin-printer-resin-viscosity-the-common-denominator/)
* [ ] Over-expose structural region of the print to add strength while letting
    edges be normally exposed

## Support for other printers

The first set of printers we'd like to support are the AnyCubic, Phrozen,
Elegoo, and Creality printers.

## Sponsors

* [Elliot from Tiny Labs](https://github.com/tinylabs)
* [@gnivler](https://github.com/gnivler)

Thank you!

## Flashing the firmware

As of now, there's no official distribution to flash the firmware via a USB
stick. You'll need:
* A programmer like JLink or [ST-Link V2](https://www.amazon.com/HiLetgo-Emulator-Downloader-Programmer-STM32F103C8T6/dp/B07SQV6VLZ) ($11)
* A 3mm hex screwdriver

## Compiling the firmware

### Prerequisites

Install the Rust toolchain. Follow instruction in the [installation section of
the Rust Embedded Book](https://docs.rust-embedded.org/book/intro/install.html).
For the target, use `thumbv7em-none-eabihf`. In a nutshell:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup component add llvm-tools-preview
cargo install cargo-binutils
# Ubuntu
sudo apt install gcc-arm-none-eabi gdb-multiarch libclang-dev openocd
# macOS
brew install armmbed/formulae/arm-none-eabi-gcc openocd
```

Finally, export this environment variable set to the root of the project.
It's for the lvgl dependency so it locates the `lv_conf.h` file.

```
export DEP_LV_CONFIG_PATH=`pwd`
```

Also you must init submodules:

```
git submodule update --init --recursive
```

### Build

```
» make build PRINTER=mono4k
cargo build --release
    Finished release [optimized + debuginfo] target(s) in 0.15s
cargo objdump --release -- -h | ./misc/rom_stats.py
.vector_table  DATA     0.3K (0.1%)
.lvgl.text     TEXT   136.3K (53.3%)
.lvgl.rodata   DATA    27.8K (10.9%)
.libs.text     TEXT    39.0K (15.2%)
.libs.rodata   DATA     8.4K (3.3%)
.text          TEXT    13.1K (5.1%)
.rodata        DATA     2.8K (1.1%)
.data          DATA     0.1K (0.1%)
.bss           BSS     65.2K (67.9%)
.uninit        BSS      0.1K (0.1%)

Total ROM        227.8K (89.0%)
Total RAM         65.4K (68.1%)
```

## Connect to the printer

First, connect your programmer to the SWD header on the board. Pinout is shown in
[Reverse engineering the Anycubic Photon Mono 4K Part1](
https://github.com/nviennot/reversing-mono4k/blob/main/writeup/part1/README.md)

Then, configure your programming interface

### JLink

Run:

```
make -j2 start_jlink start_jlink_rtt
```

### ST-Link or any OpenOCD compatible interface

* Export this variable (or edit the Makefile) to select the stlink interface:

```
export OPENOCD_INTERFACE=misc/stlink.cfg
```

* Edit `gdb/main.gdb` and pick your JLink or OpenOCD interface:

```
source ./gdb/jlink.gdb
# source ./gdb/openocd.gdb
```

* Start the OpenOCD server:

```
make start_opencd
```

### cargo-flash

You can also use [cargo-flash](https://probe.rs/docs/tools/cargo-flash/) but I
haven't tried.

## Flash the new firmware

```bash
make run
» make run
cargo run --release -q
Reading symbols from target/thumbv7em-none-eabihf/release/app...
0xf4133022 in ?? ()
Loading section .vector_table, size 0x150 lma 0x8000000
Loading section .lvgl.text, size 0x22148 lma 0x8000150
Loading section .lvgl.rodata, size 0x6f2f lma 0x8022298
Loading section .libs.text, size 0x9bfa lma 0x80291c8
Loading section .libs.rodata, size 0x217f lma 0x8032dc4
Loading section .text, size 0x3439 lma 0x8034f43
Loading section .rodata, size 0xb14 lma 0x8038380
Loading section .data, size 0x7c lma 0x8038e94
Start address 0x08034f44, load size 233225
Transfer rate: 37959 KB/sec, 12275 bytes/write.
Resetting target
A debugging session is active.

        Inferior 1 [Remote target] will be killed.

Quit anyway? (y or n) [answered Y; input not from terminal]
[main●] turbo-resin »
```

## Debugging

Use `make attach`, `c`, and `ctrl+c` to get a gdb instance connected to the device.

```
» make attach
arm-none-eabi-gdb -q -x gdb/main.gdb target/thumbv7em-none-eabihf/release/app
Reading symbols from target/thumbv7em-none-eabihf/release/app...
0xdeadbeee in ?? ()
(gdb) c
Continuing.
^C
Program received signal SIGTRAP, Trace/breakpoint trap.
0x080193ca in draw_letter_normal (map_p=0x80245fa <glyph_bitmap+4166> "", g=0x20017c88, pos=<synthetic pointer>, dsc=<optimized out>, draw_ctx=<optimized out>) at /Users/pafy/.cargo/git/checkouts/lvgl-rs-9408c72813c5388a/bf6ee92/lvgl-sys/vendor/lvgl/src/draw/sw/lv_draw_sw_letter.c:257
257                 letter_px = (*map_p & bitmask) >> (col_bit_max - col_bit);
1: x/5i $pc
=> 0x80193ca <lv_draw_sw_letter+834>:   ands    r3, r1
   0x80193cc <lv_draw_sw_letter+836>:   lsr.w   r3, r3, r12
   0x80193d0 <lv_draw_sw_letter+840>:   ands.w  r3, r3, #255    ; 0xff
   0x80193d4 <lv_draw_sw_letter+844>:   it      ne
   0x80193d6 <lv_draw_sw_letter+846>:   ldrbne.w        r3, [r11, r3]
(gdb)
```

Alternatively, you can use VSCode's _Run -> Start Debugging_ graphical debugger.

## Restore the original firmware

If you are done with your changes, you can restore the original firmware with the following:

```
make restore_rom
```

## License

Turbo Resin is licensed under the GPLv3
