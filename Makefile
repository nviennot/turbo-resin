#OPENOCD_INTERFACE ?= misc/openocd-jlink.cfg
OPENOCD_INTERFACE ?= misc/openocd-stlink.cfg

BUILD ?= debug
TARGET_ELF ?= target/thumbv7em-none-eabihf/$(BUILD)/app
BUILD_FLAGS ?=
CARGO ?= $(HOME)/.cargo/bin/cargo

#########################################################

ifeq ($(PRINTER),)
$(error PRINTER must be defined. Valid choices: mono4k, lv3)
endif

BUILD_FLAGS += --features $(PRINTER)

ifeq ($(BUILD),release)
	BUILD_FLAGS += --release
endif

ifeq (,$(wildcard $(CARGO)))
	CARGO := cargo
endif

# # We get the first string in the feature list matching the $(PRINTER)
# # variable. It's a bit gross. I wish there was a better way.
# MCU := $(shell \
# 	grep -A10000 '^\[features\]$$' Cargo.toml | \
# 	grep '^$(PRINTER)\b =' | \
# 	sed -E 's/.*\["([^"]+)".*/\1/' \
# )

ifeq ($(PRINTER),)
$(error PRINTER must be set. Try mono4k or lv3)
endif

#########################################################

.PHONY: build run attach clean start_openocd start_jlink \
	start_jlink_rtt start_probe_run_rtt restore_rom check

build:
	@# We do build first, it shows compile error messages (objdump doesn't)
	$(CARGO) build $(BUILD_FLAGS)
	$(CARGO) objdump -q $(BUILD_FLAGS) -- -h | ./misc/rom_stats.py

check:
	$(CARGO) check $(BUILD_FLAGS)

run: build
	$(CARGO) run $(BUILD_FLAGS) -q

attach:
	arm-none-eabi-gdb -q -x gdb/attach.gdb ${TARGET_ELF}

clean:
	$(CARGO) clean

start_openocd:
	openocd -f ${OPENOCD_INTERFACE} -f target/stm32f1x.cfg

start_jlink:
	JLinkGDBServer -AutoConnect 1 -Device GD32F307VE -If SWD -Speed 4000 -nogui

start_jlink_rtt:
	JLinkRTTClient

start_probe_run_rtt:
	probe-run --chip STM32F107RC --no-flash ${TARGET_ELF}

misc/orig-firmware.bin:
	@echo Dump your original firmare, and place it here: $@
	@exit 1

misc/orig-firmware.elf: misc/orig-firmware.bin
	arm-none-eabi-objcopy -I binary -O elf32-little --rename-section .data=.text --change-address 0x08000000 $< $@

restore_rom: misc/orig-firmware.elf
	arm-none-eabi-gdb -q -x gdb/run.gdb $<
