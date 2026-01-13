# Shader Presets

This directory contains curated shader presets from the libretro slang-shaders project.

## Available Presets

- **stock.slangp**: Nearest neighbor (no filtering) - default
- **xbrz-freescale.slangp**: xBRZ pixel art upscaler
- **crt-simple.slangp**: Simple CRT simulation with scanlines
- **ntsc-256px-composite.slangp**: NTSC composite video simulation

## Usage

Use the `--shader` flag to specify a shader:
```bash
neser --shader shaders/crt-simple.slangp
```

Or cycle through shaders at runtime with F6.

## Current Status

The shader infrastructure is in place:
- Shader preset loading via librashader ✓
- CLI flag and runtime cycling ✓
- Shader application in rendering pipeline (IN PROGRESS)

The actual shader rendering is currently stubbed out in `shader_manager::apply_shader()`.
To complete the implementation, we need to:

1. Modify gl_backend.rs to render NES output to an intermediate framebuffer
2. Apply shader filter chain to that framebuffer
3. Render the filtered result to the screen
4. Ensure ImGui debugger renders on top of the shader output

## Source

Shaders sourced from: https://github.com/libretro/slang-shaders
