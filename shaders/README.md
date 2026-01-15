# Shader Presets

This directory contains curated shader presets from the libretro slang-shaders project.

## Available Presets

- **stock.slangp**: Nearest neighbor (no filtering) - default
- **xbrz-freescale.slangp**: xBRZ pixel art upscaler
- **crt-lottes.slangp**: CRT simulation by Timothy Lottes (accurate scanlines, shadow mask, bloom)
- **ntsc-256px-composite.slangp**: NTSC composite video simulation

## Usage

Use the `--filter` flag to specify a shader:

```bash
neser rom.nes --filter shaders/crt-lottes.slangp
```

Or cycle through shaders at runtime with F6.

## Implementation Status

✅ **COMPLETE** - Shader infrastructure is fully functional:

- Shader preset loading via librashader ✓
- CLI flag and runtime cycling ✓
- Shader application in rendering pipeline ✓
- FilterChain::frame() properly integrated ✓
- ImGui debugger renders correctly on top ✓

The shader system is production-ready. All shaders render to the screen with hardware acceleration, and the ImGui debugger overlay continues to work correctly.

## Source

Shaders sourced from: <https://github.com/libretro/slang-shaders>
