# neser

NESER - NES Emulator in Rust

## Building

```bash
cargo build --release --features sdl
```

## Running

```bash
cargo run --release --features sdl
```

Or after building:

```bash
./target/release/neser
```

## Configuration

NESER can be configured through config files and/or command-line arguments.

### Config File Locations

Config files are loaded in the following order (later overrides earlier):

1. `~/.neser/neser.conf` - User-wide settings
2. `./neser.conf` - Directory-specific settings
3. `--config <file>` - Explicit config file (replaces steps 1 and 2)
4. Command-line arguments - Highest priority

See [neser.conf.example](neser.conf.example) for all available options with documentation.

### Quick Start

Copy the example config to get started:

```bash
# For user-wide settings
mkdir -p ~/.neser
cp neser.conf.example ~/.neser/neser.conf

# Or for directory-specific settings
cp neser.conf.example neser.conf
```

### Command-Line Options

```text
Options:
  --pal               Use PAL TV system (default: NTSC)
  --no-audio          Disable audio output
  --trace             Enable CPU trace output
  --trace-nestest     Enable CPU trace output (nestest.log format)
  --trace-ppu         Enable PPU trace output
  --trace-apu         Enable APU trace output
  --disable-pulse1    Mute pulse 1 channel
  --disable-pulse2    Mute pulse 2 channel
  --disable-triangle  Mute triangle channel
  --disable-noise     Mute noise channel
  --disable-dmc       Mute DMC channel
  --no-vsync          Disable VSync (default: enabled)
  --no-gamepads       Disable gamepad/joystick support
  --start-in-debugger Open debugger windows (CPU/PPU/APU) on startup
  --fullscreen        Run emulator in fullscreen mode
  --display <N>       Select display index for fullscreen
  --filter <path>     Specify shader preset path
  --config <path>     Specify config file path
  --video-scale <N>   Window scaling factor, windowed mode only (e.g., 4.0)
```

### Shaders

NESER supports shader presets for visual effects:

- `shaders/stock.slangp` - No effect (raw pixels)
- `shaders/crt-lottes.slangp` - CRT simulation (scanlines, shadow mask, bloom)
- `shaders/xbrz-freescale.slangp` - Smooth pixel upscaling
- `shaders/ntsc-256px-composite.slangp` - NTSC composite video simulation

Example:

```bash
neser --filter shaders/crt-lottes.slangp
```

Or in config file:

```text
filter=shaders/crt-lottes.slangp
```
