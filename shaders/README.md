# Shader Presets

This directory contains curated shader presets that reference the [libretro/slang-shaders](https://github.com/libretro/slang-shaders) repository, which is included as a git submodule at `vendor/slang-shaders`.

## Setup

After cloning neser, initialise the submodule to enable the non-stock shaders:

```bash
git submodule update --init
```

Or clone with submodules from the start:

```bash
git clone --recurse-submodules https://github.com/rmstdope/neser
```

## Available Presets

- **stock.slangp**: Nearest neighbor (no filtering) — no submodule required
- **xbrz-freescale.slangp**: xBRZ pixel art upscaler (requires submodule)
- **crt-lottes.slangp**: CRT simulation by Timothy Lottes — scanlines, shadow mask, bloom (requires submodule, public domain)
- **ntsc-256px-composite.slangp**: NTSC composite video simulation (requires submodule)

## Usage

Use the `--nes-filter` flag for NES, `--gb-filter` for Game Boy, or
`--gba-filter` for Game Boy Advance:

```bash
neser rom.nes --nes-filter crt     # CRT simulation
neser rom.nes --nes-filter ntsc    # NTSC composite
neser rom.nes --nes-filter smooth  # Smooth upscaling
neser rom.nes --nes-filter none    # No filter
neser rom.gb  --gb-filter dmg  # DMG dot-matrix LCD
neser rom.gba --gba-filter gba-lcd        # GBA LCD shader
neser rom.gba --gba-filter agb001         # AGB-001 style handheld look
neser rom.gba --gba-filter nso-gba-color  # NSO color mod preset
neser rom.gba --gba-filter sp101-color    # SP-101 color mod preset
neser rom.gba --gba-filter gba-lcd-grid   # GBA LCD grid with border
```

Or set in config file:

```text
nes-filter=crt
```

You can also cycle through shaders at runtime with F4.
For GBA, cycling follows this order: none, gba-lcd, agb001,
nso-gba-color, sp101-color, gba-lcd-grid.

## Why a submodule?

The shader files carry per-file licenses (public domain, MIT, GPL-3.0).
Rather than vendoring the source directly in this repository, a shallow
submodule is used so that each shader is fetched from the authoritative
upstream repository along with its original license text.
