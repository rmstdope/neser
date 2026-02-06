# Shader Filters

NESER supports custom shader filters for visual effects. These filters are implemented using custom GLSL shaders in both the SDL and web frontends.

## Available Filters

- **stock**: No filtering (nearest neighbor) - default
- **smooth**: Smooth (bilinear/trilinear filtering)
- **crt**: CRT simulation (scanlines, shadow mask, bloom, screen warp)
- **ntsc**: NTSC composite video simulation (YIQ encoding with chroma artifacts)

## Implementation

**SDL Frontend**: Custom OpenGL shaders compiled at runtime from GLSL source embedded in the binary.

**Web Frontend**: Custom WebGL shaders with the same visual effects.

Both frontends use custom shader implementations to avoid external dependencies and ensure consistent behavior across platforms.

## Usage

Via command line:
```bash
neser --filter crt
neser --filter ntsc
neser --filter smooth
neser --filter none
```

Via config file:
```
filter=crt
```

Or cycle through filters at runtime by pressing F6.

## Technical Details

The shader presets (.slangp files) in this directory are retained for backward compatibility but are no longer required. The actual shader implementation is now in:
- SDL: `src/rendering/shader_programs.rs` (GLSL source) and `src/rendering/shader_manager.rs` (compilation and management)
- Web: `web/app.js` (WebGL shader source and rendering)

## Filter Order

When cycling filters with F6, the order is:
1. None (stock)
2. NTSC
3. CRT
4. Smooth
