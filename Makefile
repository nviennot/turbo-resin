.PHONY: build run attach start_opencd start_jlink start_jlink_rtt restore_rom

OPENOCD_INTERFACE ?= stuff/openocd-jlink.cfg

build:
	@# build --release first, it shows compile error messages
	cargo build --release
	cargo objdump -q --release -- -h | ./stuff/rom_stats.py

run: build
	cargo run --release -q

attach:
	arm-none-eabi-gdb -q -x gdb/main.gdb target/thumbv7em-none-eabihf/release/app

start_openocd:
	openocd -f ${OPENOCD_ADAPTER_CFG} -f target/stm32f1x.cfg

start_jlink:
	JLinkGDBServer -AutoConnect 1 -Device GD32F307VE -If SWD -Speed 4000 -nogui

start_jlink_rtt:
	JLinkRTTClient

stuff/orig-firmware.elf: stuff/orig-firmware.bin
	arm-none-eabi-objcopy -I binary -O elf32-little --rename-section .data=.text --change-address 0x08000000 $< $@

restore_rom: stuff/orig-firmware.elf
	arm-none-eabi-gdb -q -x gdb/run.gdb $<
