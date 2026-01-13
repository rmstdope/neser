# Shader Presets

This directory contains curated shader presets from the libretro slang-shaders project.

## Available Presets

- **stock.slangp**: Nearest neighbor (no filtering) - default
- **xbrz-freescale.slangp**: xBRZ pixel art upscaler
- **crt-simple.slangp**: Simple CRT simulation with scanlines
- **ntsc-256px-composite.slangp**: NTSC composite video simulation

## Usage

Use the `--shader` flag to specify a shader:
```
neser --shader shaders/crt-simple.slangp
```

Or cycle through shaders at runtime with F6.

## Source

Shaders sourced from: https://github.com/libretro/slang-shaders
