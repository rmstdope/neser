# neser

NESER - NES Emulator in Rust

## Installation

### Pre-built binaries (recommended)

Download the latest release for your platform from the [GitHub Releases](https://github.com/rmstdope/neser/releases) page.

### From source via cargo install

```bash
cargo install neser
```

No SDL2 setup is required — the native frontend uses Rust crates for windowing (winit), audio (cpal), and gamepad input (gilrs).
On Linux, building from source may still require system development packages for backend libraries used by those crates, such as ALSA and Wayland/X11.

## Building

```bash
cargo build --release
```

## Development Setup

After cloning, activate the shared git hooks (runs `cargo fmt` automatically before each commit):

```bash
git config core.hooksPath .githooks
```

## Running

```bash
cargo run --release
```

When launched without a ROM path, NESER opens the **ROM Browser** — a console-style graphical launcher with cover art grid, search, genre filtering, detail view, and favorites.

To launch a specific ROM directly (skipping the browser):

```bash
cargo run --release -- path/to/rom.nes
```

### Native emulator keyboard controls

NES and Game Boy use the same primary layout: `W/A/S/D` for the D-pad, `R` for B,
`T` for A, `4` for Select, and `5` for Start. Game Boy and Game Boy Advance also
accept the arrow keys as D-pad aliases. Game Boy Advance adds `Q` for L and `E`
for R.

NES player 2 uses `I/J/K/L` for the D-pad, `P` for B, `O` for A, `9` for Select,
and `0` for Start.

### ROM Browser

The ROM browser scans configured search paths for NES ROMs and displays them in a cover art grid with metadata from TheGamesDB.

**Keyboard shortcuts:**
- Arrow keys / D-pad: Navigate the grid
- Enter: Open detail view for selected ROM (launch from detail view)
- Tab: Toggle search overlay (type to filter in real-time)
- Escape: Open filter panel (platform, players, genre)
- `g`: Open genre filter overlay
- `d`: Open detail view for selected ROM
- `f`: Toggle favorite on selected ROM
- `F`: Toggle favorites-only filter
- Ctrl+F: Toggle fullscreen
- Ctrl+Q: Quit browser

**Setup for cover art:**
1. Download `metadata.db` from TheGamesDB and place it at `~/.neser/metadata.db` (or configure `metadata-db-path`)
2. Cover art images are automatically downloaded to `~/.neser/image_cache/` on first launch

### Autorunner

Record or play back joypad input alongside a ROM:

```bash
cargo run --release -- --record roms/games/pac-man.nes
cargo run --release -- --playback roms/games/pac-man.nes
cargo run --release -- --playback --headless roms/games/pac-man.nes
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
  --nes-filter <name>   Specify NES shader filter (crt|ntsc|smooth|pal|none)
  --gb-filter <name>    Specify Game Boy shader filter (dmg|none)
  --gba-filter <name>   Specify Game Boy Advance shader filter (none|gba-lcd|agb001|nso-gba-color|sp101-color|gba-lcd-grid)
  --gba-bios-path <path>
                         Path to external GBA BIOS file (optional; overrides built-in BIOS)
  --config <path>       Specify config file path (overrides default locations)
  --window-height <N>   Window height in pixels, windowed mode only (e.g., 720)
```

NESER includes a built-in open-source GBA BIOS, so no external BIOS file is needed
to run GBA ROMs. To use a proprietary BIOS dump instead, provide a 16384-byte BIOS
file via `--gba-bios-path` or config key `gba-bios-path`. If unset, NESER also
checks `~/.neser/gba_bios.bin` before falling back to the built-in BIOS.

### GBA Automated CPU Suite Tests

The repository includes the jsmolka `gba-tests` suite as a submodule at
`roms/gba/automated_tests/gba-tests`.

Run the ARM/Thumb/Memory GBA suite integration tests with:

```bash
cargo test --no-default-features --lib gba::integration_tests::gba_suite_tests::
```

These tests execute:

- `roms/gba/automated_tests/gba-tests/arm/arm.gba`
- `roms/gba/automated_tests/gba-tests/thumb/thumb.gba`
- `roms/gba/automated_tests/gba-tests/memory/memory.gba`

Pass/fail is read from suite-native registers at completion:

- ARM uses `R12`
- Thumb uses `R7`
- Memory uses `R12`

## Controller Mapping

NESER supports two controller ports (port 1 and port 2), each of which can be configured with different controller types.

### Supported Controller Types

- **Joypad** - Standard NES controller (default for both ports)
- **Paddle** - Arkanoid-style paddle controller with analog position and trigger button

### Per-Port Controller Selection

Each port can be independently configured in the configuration file or via the auto-detection mechanism:

**Port 1** (Controller 1):
- Default: Joypad
- Can be configured as Paddle for games that use paddle on port 1

**Port 2** (Controller 2):
- Default: Joypad  
- Can be configured as Paddle for games that use paddle on port 2

### Four Score (4-player) mode

Enable Four Score with CLI `--enable-4-score` or config key `enable_4_score=true`.

When enabled, input is assigned as follows:

- 2 physical gamepads connected: gamepads control emulated players 1-2, keyboard controls players 3-4
- 1 physical gamepad connected: gamepad controls player 1, keyboard controls players 2-3
- 0 physical gamepads connected: keyboard controls players 1-2

### Auto-Detection

NESER automatically detects known games that require special controllers (for example, paddle or Zapper) by ROM CRC32 and configures the appropriate port **only when neither controller port is explicitly configured**:

- **Arkanoid** (CRC: 0x32FB0583) → Paddle on **port 2**
- **PaddleTest3** (CRC: 0x47F9F410) → Paddle on **port 1**
- Known Zapper-based games are similarly detected and configured with a Zapper on the appropriate port

**Important**: Any explicit controller configuration (`controller_port1` and/or `controller_port2`, including CLI overrides) disables CRC-based auto-detection so your configured controllers are always used.

When a known ROM is loaded and no controller ports are explicitly configured, you'll see a message like:
```
Enabling Arkanoid controller on port 2 for inserted cartridge
```

### Paddle Controller Input

When a paddle controller is enabled on either port:

- **Mouse X position** controls the paddle position (non-linear curve with faster edge response)
- **Left mouse button** controls the paddle trigger
- The mouse input is routed to all ports configured as paddles
- Keyboard/gamepad input for that port is suppressed while paddle mode is active

### Manual Configuration Examples

Manual controller configuration via config file is supported and will override auto-detection. Configure controller types in your config file before loading a ROM:

**Example 1: Paddle on Port 1**
```text
controller_port1=arkanoid
controller_port2=joypad
```

**Example 2: Paddle on Port 2 (Arkanoid)**
```text
controller_port1=joypad
controller_port2=arkanoid
```

**Example 3: Paddles on Both Ports**
```text
controller_port1=arkanoid
controller_port2=arkanoid
```

**Note**: When you explicitly configure controllers, auto-detection is disabled, giving you full control over controller configuration regardless of which ROM is loaded.

### Validation Steps

1. Run the emulator with a paddle-enabled ROM:
   - roms/games/arkanoid.nes (if available)
   - roms/automated_tests/PaddleTest3 (if available)
2. Move the mouse left/right and confirm the paddle moves across the full range.
3. Verify fine control near the center and faster response near the edges.
4. Click and release the left mouse button and confirm the trigger is detected.
5. Confirm keyboard/controller input for that port does not interfere while paddle mode is active.

### Blargg NES Test Suite Status

NESER is tested against Blargg’s (and Blargg compatible) NES test ROMs to verify CPU, PPU, APU, DMA, and mapper correctness. The following test suites are currently passing in the main branch:

**Test Suites (by directory and ROM file):**

- **apu_mixer/**
  - dmc.nes
  - noise.nes
  - triangle.nes
  - square.nes
- **apu_reset/**
  - 4015_cleared.nes
  - 4017_timing.nes
  - 4017_written.nes
  - irq_flag_cleared.nes
  - len_ctrs_enabled.nes
  - works_immediately.nes
- **apu_test/rom_singles/**
  - 1-len_ctr.nes
  - 2-len_table.nes
  - 3-irq_flag.nes
  - 4-jitter.nes
  - 5-len_timing.nes
  - 6-irq_flag_timing.nes
  - 7-dmc_basics.nes
  - 8-dmc_rates.nes
- **dmc_dma_during_read4/**
  - dma_2007_read.nes
  - dma_2007_write.nes
  - dma_4016_read.nes
  - double_2007_read.nes
  - read_write_2007.nes
- **blargg_ppu_tests_2005.09.15b/**
  - palette_ram.nes
  - sprite_ram.nes
  - vbl_clear_time.nes
  - vram_access.nes
- **branch_timing_tests/**
  - 1.Branch_Basics.nes
  - 2.Backward_Branch.nes
  - 3.Forward_Branch.nes
- **cpu_dummy_reads/**
  - cpu_dummy_reads.nes
- **cpu_dummy_writes/**
  - cpu_dummy_writes_oam.nes
  - cpu_dummy_writes_ppumem.nes
- **cpu_exec_space/**
  - test_cpu_exec_space_ppuio.nes
  - test_cpu_exec_space_apu.nes
- **cpu_interrupts_v2/rom_singles/**
  - 1-cli_latency.nes
  - 2-nmi_and_brk.nes
  - 3-nmi_and_irq.nes
  - 4-irq_and_dma.nes
  - 5-branch_delays_irq.nes
- **cpu_reset/**
  - registers.nes
  - ram_after_reset.nes
- **cpu_timing_test6/**
  - cpu_timing_test.nes
- **instr_misc/rom_singles/**
  - 01-abs_x_wrap.nes
  - 02-branch_wrap.nes
  - 03-dummy_reads.nes
  - 04-dummy_reads_apu.nes
- **instr_test-v5/rom_singles/**
  - 01-basics.nes
  - 02-implied.nes
  - 03-immediate.nes
  - 04-zero_page.nes
  - 05-zp_xy.nes
  - 06-absolute.nes
  - 07-abs_xy.nes
  - 08-ind_x.nes
  - 09-ind_y.nes
  - 10-branches.nes
  - 11-stack.nes
  - 12-jmp_jsr.nes
  - 13-rts.nes
  - 14-rti.nes
  - 15-brk.nes
  - 16-special.nes
- **instr_timing/rom_singles/**
  - 1-instr_timing.nes
  - 2-branch_timing.nes
- **mmc3_irq_tests/**
  - 1.Clocking.nes
  - 2.Details.nes
  - 3.A12_clocking.nes
  - 4.Scanline_timing.nes
  - 5.MMC3_rev_A.nes
  - 6.MMC3_rev_B.nes
- **mmc3_test_2/rom_singles/**
  - 1-clocking.nes
  - 2-details.nes
  - 3-A12_clocking.nes
  - 4-scanline_timing.nes
  - 5-MMC3.nes
  - 6-MMC3_alt.nes
- **oam_read/**
  - oam_read.nes
- **oam_stress/**
  - oam_stress.nes
- **ppu_open_bus/**
  - ppu_open_bus.nes
- **ppu_read_buffer/**
  - test_ppu_read_buffer.nes
- **ppu_sprite_hit/rom_singles/**
  - 01-basics.nes
  - 02-alignment.nes
  - 03-corners.nes
  - 04-flip.nes
  - 05-left_clip.nes
  - 06-right_edge.nes
  - 07-screen_bottom.nes
  - 08-double_height.nes
  - 09-timing.nes
  - 10-timing_order.nes
- **ppu_sprite_overflow/rom_singles/**
  - 01-basics.nes
  - 02-details.nes
  - 03-timing.nes
  - 04-obscure.nes
  - 05-emulator.nes
- **ppu_vbl_nmi/rom_singles/**
  - 01-vbl_basics.nes
  - 02-vbl_set_time.nes
  - 03-vbl_clear_time.nes
  - 04-nmi_control.nes
  - 05-nmi_timing.nes
  - 06-suppression.nes
  - 07-nmi_on_timing.nes
  - 08-nmi_off_timing.nes
  - 09-even_odd_frames.nes
  - 10-even_odd_timing.nes
- **sprdma_and_dmc_dma/**
  - sprdma_and_dmc_dma.nes
  - sprdma_and_dmc_dma_512.nes

> See src/blargg_tests.rs for the full list of passing tests and ROM paths.

## Shaders

NESER supports shader filters for visual effects:

- `crt`    - CRT simulation (scanlines, shadow mask, bloom)
- `ntsc`   - NTSC composite video simulation
- `smooth` - Smooth pixel upscaling (xBRZ)
- `none`   - No effect (raw pixels)

Example:

```bash
neser --nes-filter crt
```

Or in config file:

```text
filter=crt
```
