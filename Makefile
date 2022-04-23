BUILD ?= debug
TARGET_ELF ?= target/thumbv7em-none-eabihf/$(BUILD)/app
BUILD_FLAGS ?=
CARGO ?= $(HOME)/.cargo/bin/cargo
#OPENOCD_INTERFACE ?= misc/openocd-jlink.cfg
OPENOCD_INTERFACE ?= misc/openocd-stlink.cfg

#########################################################

export DEP_LV_CONFIG_PATH := $(PWD)

BUILD_FLAGS += --features $(PRINTER)

ifeq ($(BUILD),release)
	BUILD_FLAGS += --release
endif

ifeq (,$(wildcard $(CARGO)))
	CARGO := cargo
endif

# We get the first string in the feature list matching the $(PRINTER)
# variable. It's a bit gross. I wish there was a better way.
export MCU := $(shell \
	grep -A10000 '^\[features\]$$' Cargo.toml | \
	grep '^$(PRINTER)\b =' | \
	sed -E 's/.*\["([^"]+)".*/\1/' \
)

#########################################################

.PHONY: build run attach clean start_openocd start_jlink \
	start_jlink_rtt start_probe_run_rtt restore_rom check \
	check_printer check_submodules

check_printer:
ifeq (${MCU},)
	$(error Try with PRINTER=mono4k or PRINTER=lv3 PRINTER=saturn)
endif

embassy/stm32-data/README.md:
	git submodule update --init --recursive

check_submodules: embassy/stm32-data/README.md
	git submodule update --recursive

build: | check_printer check_submodules
	@# We do build first, it shows compile error messages (objdump doesn't)
	$(CARGO) build $(BUILD_FLAGS)
	$(CARGO) objdump -q $(BUILD_FLAGS) -- -h | ./misc/rom_stats.py

check: | check_printer check_submodules
	$(CARGO) check $(BUILD_FLAGS)

run: build
	$(CARGO) run $(BUILD_FLAGS) -q

attach:
	arm-none-eabi-gdb -q -x gdb/attach.gdb ${TARGET_ELF}

attach_bare:
	arm-none-eabi-gdb -q -x gdb/attach.gdb

clean:
	$(CARGO) clean

start_openocd: | check_printer
ifeq (${MCU},stm32f407ze)
	openocd -f ${OPENOCD_INTERFACE} -f target/stm32f4x.cfg
else
	openocd -f ${OPENOCD_INTERFACE} -f target/stm32f1x.cfg
endif

start_jlink: | check_printer
	JLinkGDBServer -Device ${MCU} -If SWD -Speed 4000 -nogui

start_jlink_rtt:
	JLinkRTTClient

start_probe_run_rtt: | check_printer
	probe-run --chip ${MCU} --no-flash ${TARGET_ELF}

misc/orig-firmware-$(PRINTER).bin:
	@echo Dump your original firmare, and place it here: $@
	@echo To do this, you can run 'make start_openocd', then:
	@echo 'echo dump_image $@ 0 $$((512*1024)) | nc localhost 4444'
	@exit 1

misc/orig-firmware-$(PRINTER).elf: misc/orig-firmware-$(PRINTER).bin
	arm-none-eabi-objcopy -I binary -O elf32-little --rename-section .data=.text --change-address 0x08000000 $< $@

restore_rom: misc/orig-firmware-$(PRINTER).elf | check_printer
	arm-none-eabi-gdb -q -x gdb/run.gdb $<
