# GBA Open-Source BIOS

An open-source replacement for the proprietary GBA BIOS, written in ARM assembly.

## Building

Prerequisites:
- `arm-none-eabi-as` (GNU ARM assembler)
- `arm-none-eabi-ld` (GNU ARM linker)
- `arm-none-eabi-objcopy` (GNU ARM object copy)

### Install toolchain

**macOS:**
```bash
brew install --cask gcc-arm-embedded
```

**Ubuntu/Debian:**
```bash
sudo apt-get install binutils-arm-none-eabi
```

### Build

```bash
cd src/gba/bios
make
```

This produces `bios.bin` (exactly 16384 bytes), which is committed to the repo.

## Pre-built Binary

The pre-built `bios.bin` is committed alongside the source so that developers
don't need the ARM toolchain unless they're modifying the BIOS assembly.

## Architecture

The BIOS implements:
- Exception vector table (reset, SWI, IRQ, traps)
- Minimal boot sequence (stack setup, jump to cartridge)
- IRQ dispatcher (reads handler from 0x03FFFFFC)
- SWI functions: SoftReset (0x00), RegisterRamReset (0x01), Halt (0x02),
  Stop (0x03), IntrWait (0x04), VBlankIntrWait (0x05), Div (0x06),
  DivArm (0x07), Sqrt (0x08), BiosChecksum (0x0D)

## License

MIT License - same as the parent project.
