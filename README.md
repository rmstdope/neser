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

### Autorunner

Record or play back joypad input alongside a ROM:

```bash
cargo run --release --features sdl --bin autorunner -- --record roms/games/pac-man.nes
cargo run --release --features sdl --bin autorunner -- --playback roms/games/pac-man.nes
```

Or after building:

```bash
./target/release/neser
```

## Configuration


NESER can be configured via config files and/or command-line arguments.

### Config File Locations & Priority

Configuration is loaded with the following priority (highest overrides lowest):

1. Default values (built-in)
2. `~/.neser/neser.conf` (user-wide settings, if exists)
3. `./neser.conf` (project/directory-specific, if exists)
4. `--config <file>` (if specified, replaces steps 2 and 3)
5. Command-line arguments (highest priority)

See [neser.conf.example](neser.conf.example) for all available options and documentation.

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
  --help                Show help message
  -h                    Show help message (short)
  --pal                 Use PAL TV system (default: NTSC)
  --no-audio            Disable audio output
  --trace               Enable CPU trace output (level 1)
  --trace-nestest       Enable CPU trace output (nestest.log format)
  --trace-cpu[=N]       Enable CPU trace output at level N (default: 1)
  --trace-ppu[=N]       Enable PPU trace output at level N (default: 1)
  --trace-apu[=N]       Enable APU trace output at level N (default: 1)
  --trace-mapper[=N]    Enable Mapper trace output at level N (default: 1)
    #
    # Trace level N (0 = off, 5 = VERY verbose/detailed)
    # Example: --trace-cpu=2 or --trace-ppu=3
  --disable-pulse1      Mute pulse 1 channel
  --disable-pulse2      Mute pulse 2 channel
  --disable-triangle    Mute triangle channel
  --disable-noise       Mute noise channel
  --disable-dmc         Mute DMC channel
  --no-vsync            Disable VSync (default: enabled)
  --no-gamepads         Disable gamepad/joystick support
  --start-in-debugger   Open debugger windows (CPU/PPU/APU) on startup (starts paused)
  --load-state <path>   Load a save state on startup
  --fullscreen          Run emulator in fullscreen mode
  --display <N>         Select display index for fullscreen (default: 0)
  --filter <path>       Specify shader preset path (see Shaders below)
  --config <path>       Specify config file path (overrides default locations)
  --video-scale <N>     Window scaling factor, windowed mode only (e.g., 4.0)
```


### Shaders

NESER supports shader presets for visual effects:

- `shaders/stock.slangp`               No effect (raw pixels)
- `shaders/crt-lottes.slangp`          CRT simulation (scanlines, shadow mask, bloom)
- `shaders/xbrz-freescale.slangp`      Smooth pixel upscaling
- `shaders/ntsc-256px-composite.slangp` NTSC composite video simulation

Example:

```bash
neser --filter shaders/crt-lottes.slangp
```

Or in config file:

```text
filter=shaders/crt-lottes.slangp
```
