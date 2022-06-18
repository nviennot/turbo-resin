# Pick your printer
# Don't forget to modify ./vscode/settings.json as well if you are using VSCode.
PRINTER ?= mono4k
#PRINTER ?= saturn

# Pick your hardawre probe by specifying the PROBE variable.
# Then set the FLASH_WITH variable appropriately.
# Combos (PROBE, FLASH_WITH) that work:
# * (jlink, jlink+gdb)
# * (jlink, openocd+gdb)
# * (jlink, probe-run)
# * (stlink, openocd+gdb)
# * (stlink, probe-run)

PROBE ?= jlink
#PROBE ?= stlink

FLASH_WITH ?= jlink+gdb
#FLASH_WITH ?= openocd+gdb
# To use probe-run, you need to run `cargo install probe-run` first.
#FLASH_WITH ?= probe-run

# If you'd like to tell gdb to load the MCU's SVD file to help with debugging
# registers, you may provide a copy of https://github.com/bnahill/PyCortexMDebug
GDB_SVD_LOAD ?=
# GDB_SVD_LOAD ?= ./repos/PyCortexMDebug/scripts/gdb.py

#########################################################

BUILD ?= debug
CARGO ?= $(HOME)/.cargo/bin/cargo
GDB ?= arm-none-eabi-gdb -q
OBJCOPY ?= arm-none-eabi-objcopy

OPENOCD_INTERFACE ?= misc/openocd-$(PROBE).cfg
TARGET_ELF ?= target/thumbv7em-none-eabihf/$(BUILD)/app
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

export DEP_LV_CONFIG_PATH := $(PWD)/lv_conf/$(shell \
	grep LVCONF_PATH src/consts/$(PRINTER).rs | \
	sed -E 's/.*=.*"(.*)".*/\1/' \
)

ifeq ($(FLASH_WITH),openocd+gdb)
	GDB += -x gdb/openocd.gdb
else
	GDB += -x gdb/jlink.gdb
endif

ifneq ($(GDB_SVD_LOAD),)
	GDB += -x $(GDB_SVD_LOAD) -ex "svd_load ./misc/$(MCU).svd"
endif

# probe-run doesn't know about the gd32f307ve. We'll fall back on a close stm32
# equivalent in terms of flash size.

ifeq (${MCU},gd32f307ve)
	PROBE_RUN_CHIP ?= stm32f103ze
else
	PROBE_RUN_CHIP ?= $(MCU)
endif

ifeq (${MCU},stm32f407ze)
	OPENOCD_TARGET ?= target/stm32f4x.cfg
else
	OPENOCD_TARGET ?= target/stm32f1x.cfg
endif

#########################################################

.PHONY: build check flash run attach clean start_probe start_probe_rtt \
	restore_rom check_submodules

all: build;

embassy/stm32-data/README.md:
	git submodule update --init --recursive

check_submodules: embassy/stm32-data/README.md
	git submodule update --recursive

build: | check_submodules
	@# We do build first, it shows compile error messages (objdump doesn't)
	$(CARGO) build $(BUILD_FLAGS)
	$(CARGO) objdump -q $(BUILD_FLAGS) -- -h | ./misc/rom_stats.py

check: | check_submodules
	$(CARGO) check $(BUILD_FLAGS)

flash: $(TARGET_ELF)
ifeq (${FLASH_WITH},probe-run)
	probe-run --chip $(PROBE_RUN_CHIP) $<
else
	$(GDB) -x gdb/run.gdb $<
endif

run: build flash;

attach:
	$(GDB) -x gdb/attach.gdb ${TARGET_ELF}

attach_bare:
	$(GDB) -x gdb/attach.gdb

clean:
	$(CARGO) clean

start_probe:
ifeq (${FLASH_WITH},jlink+gdb)
	JLinkGDBServer -Device ${MCU} -If SWD -Speed 4000 -nogui
else ifeq (${FLASH_WITH},openocd+gdb)
	openocd -f ${OPENOCD_INTERFACE} -f ${OPENOCD_TARGET}
else
	@echo "No need to 'make start_probe' when using probe-run, and debugging is not supported"
endif

start_probe_rtt:
ifeq (${FLASH_WITH},jlink+gdb)
	JLinkRTTClient
else
	probe-run --chip ${PROBE_RUN_CHIP} --no-flash ${TARGET_ELF}
endif

misc/orig-firmware-$(PRINTER).bin:
	@echo Dump your original firmare, and place it here: $@
	@echo To do this, run 'make start_probe FLASH_WITH=openocd+gdb', then:
	@echo 'echo dump_image $@ 0 $$((512*1024)) | nc localhost 4444'
	@exit 1

misc/orig-firmware-$(PRINTER).elf: misc/orig-firmware-$(PRINTER).bin
	$(OBJCOPY) -I binary -O elf32-little --rename-section .data=.text --change-address 0x08000000 $< $@

restore_rom: misc/orig-firmware-$(PRINTER).elf
ifeq (${FLASH_WITH},probe-run)
	probe-run --chip $(PROBE_RUN_CHIP) $<
else
	$(GDB) -x gdb/run.gdb $<
endif
