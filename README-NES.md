# NES support in NESER

This file covers NES-specific usage. For installation, generic build/run commands, configuration file locations, and development setup, see [README.md](README.md).

## Supported hardware modes

NESER supports several NES-family timing modes:

- `nes-ntsc`
- `nes-pal`
- `famicom`
- `dendy`
- `playchoice10` / `playchoice-10`

Select a mode on the command line:

```bash
neser --nes-hardware nes-pal path/to/game.nes
```

Or in `neser.conf`:

```text
nes-hardware=nes-pal
```

## Running NES ROMs

```bash
neser path/to/game.nes
cargo run --release --bin neser -- path/to/game.nes
```

NESER also provides a ROM browser when launched without a ROM path. The browser can scan configured cartridge search paths, show metadata and cover art, filter entries, and launch selected ROMs.

Relevant catalog settings are documented in [neser.conf.example](neser.conf.example), including metadata database paths, image cache paths, unofficial-ROM filtering, and cartridge search paths.

## Input and controllers

Default keyboard mapping for standard NES controllers:

| Control | Player 1 | Player 2 |
| --- | --- | --- |
| D-pad | `W`/`A`/`S`/`D` | `I`/`J`/`K`/`L` |
| A | `T` | `O` |
| B | `R` | `P` |
| Select | `4` | `9` |
| Start | `5` | `0` |

The native frontend also supports gamepads through `gilrs`.

NES-specific controller features include:

- Standard joypads
- SNES controllers
- SNES Mouse
- Zapper
- Arkanoid paddle
- Power Pad
- Four Score
- Famicom expansion-port controllers
- VS System and PlayChoice-10 inputs

Controller selection is configured with keys such as:

```text
nes-controller_port1=joypad
nes-controller_port2=joypad
nes-expansion-port=none
nes-enable_4_score=false
```

See [neser.conf.example](neser.conf.example) for the current accepted values and additional controller options.

## Video and shaders

NES shader presets are selected with `--nes-filter` or `nes-filter`:

```bash
neser --nes-filter crt path/to/game.nes
```

Documented presets include:

- `crt`
- `ntsc`
- `smooth`
- `pal`
- `none`

NES overscan removal is controlled with:

```text
nes-horizontal_overscan=8
nes-vertical_overscan=8
```

## Debugging and autorun

NESER includes debugger and trace options useful for NES development:

```bash
neser --debugger path/to/game.nes
neser --trace-cpu path/to/game.nes
neser --breakpoint pc=C000 path/to/game.nes
```

Autorun recording/playback is available for deterministic input workflows:

```bash
neser --create-recording path/to/game.nes
neser --playback path/to/game.nes
neser --playback-headless path/to/game.nes
```

Use `neser --help` for the complete current CLI reference.

## Automated NES tests

NES automated tests live under:

- `src/nes/integration_tests/`
- `roms/nes/automated_tests/`
- `roms/automated_tests/`

They cover CPU behavior, PPU behavior, APU/audio behavior, DMA, input devices, mapper behavior, autorun workflows, RAM initialization, and assorted regression ROMs.

Useful commands:

```bash
./scripts/test-dir.sh src/nes --skip-integration
./scripts/test-dir.sh src/nes/cartridge
cargo test --no-default-features --lib nes::integration_tests::
```

For source-level architecture details, see [architecture.md](architecture.md).
