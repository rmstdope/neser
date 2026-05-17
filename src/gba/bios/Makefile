# GBA Open-Source BIOS - Build System
#
# Prerequisites: arm-none-eabi-as, arm-none-eabi-ld, arm-none-eabi-objcopy
# Install on macOS: brew install --cask gcc-arm-embedded
# Install on Ubuntu: sudo apt-get install binutils-arm-none-eabi

PREFIX ?= arm-none-eabi-
AS = $(PREFIX)as
LD = $(PREFIX)ld
OBJCOPY = $(PREFIX)objcopy

BIOS_SIZE = 16384

.PHONY: all clean

all: bios.bin

bios.o: bios.s
	$(AS) -mcpu=arm7tdmi -o $@ $<

bios.elf: bios.o bios.ld
	$(LD) -T bios.ld -o $@ $<

bios.bin: bios.elf
	$(OBJCOPY) -O binary $< $@
	@# Pad to exactly 16384 bytes
	@SIZE=$$(wc -c < $@); \
	if [ $$SIZE -lt $(BIOS_SIZE) ]; then \
		dd if=/dev/zero bs=1 count=$$(($(BIOS_SIZE) - $$SIZE)) >> $@ 2>/dev/null; \
	elif [ $$SIZE -gt $(BIOS_SIZE) ]; then \
		echo "ERROR: BIOS too large ($$SIZE > $(BIOS_SIZE) bytes)" >&2; \
		rm -f $@; \
		exit 1; \
	fi
	@echo "Built bios.bin ($$(wc -c < $@ | tr -d ' ') bytes)"

clean:
	rm -f bios.o bios.elf bios.bin
