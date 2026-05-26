# Game Boy Advance support in NESER

This file covers Game Boy Advance-specific usage. For installation, generic build/run commands, configuration file locations, and development setup, see [README.md](README.md).

## Supported hardware models

NESER exposes these GBA hardware model settings:

- `agb`
- `sp`
- `micro`

Example:

```bash
neser --gba-hardware sp path/to/game.gba
```

Or in `neser.conf`:

```text
gba-hardware=sp
```

## Running GBA ROMs

```bash
neser path/to/game.gba
cargo run --release --bin neser -- path/to/game.gba
```

Use `neser --help` for the complete current CLI reference.

## BIOS behavior

NESER includes a built-in open-source GBA BIOS, so an external BIOS file is not required.

BIOS selection order:

1. `--gba-bios-path <path>` / `gba-bios-path=<path>` if configured
2. `~/.neser/gba_bios.bin` if it exists
3. Built-in BIOS fallback

The external BIOS must be exactly 16 KiB. Use the special value `embedded` to force the built-in BIOS:

```bash
neser --gba-bios-path embedded path/to/game.gba
```

Skip only the visual/audio BIOS intro while preserving hardware initialization:

```bash
neser --skip-bios-intro path/to/game.gba
```

## Input

Default keyboard mapping:

| GBA button | Keyboard |
| --- | --- |
| D-pad | `W`/`A`/`S`/`D` or arrow keys |
| A | `T` |
| B | `R` |
| L | `Q` |
| R | `E` |
| Select | `4` |
| Start | `5` |

The native frontend also supports gamepads through `gilrs`.

## Video and color

GBA shader presets are selected with `--gba-filter` or `gba-filter`:

```bash
neser --gba-filter gba-lcd path/to/game.gba
```

Documented presets include:

- `none`
- `gba-lcd`
- `agb001`
- `nso-gba-color`
- `sp101-color`
- `gba-lcd-grid`

GBA LCD color correction can be enabled with:

```bash
neser --gba-color-correction path/to/game.gba
```

Or in `neser.conf`:

```text
gba-color-correction=true
```

## Diagnostics

GBA trace channels are available for emulator diagnostics:

```bash
neser --gba-trace-cpu path/to/game.gba
neser --gba-trace-bus path/to/game.gba
neser --gba-trace-dma path/to/game.gba
neser --gba-trace-swi path/to/game.gba
neser --gba-trace-mgba-log path/to/game.gba
```

## Automated GBA tests

GBA automated tests live under:

- `src/gba/integration_tests/`
- `roms/gba/automated_tests/`

The repository includes the jsmolka `gba-tests` suite as a submodule at:

```text
roms/gba/automated_tests/gba-tests
```

Run the ARM/Thumb/Memory suite tests with:

```bash
cargo test --no-default-features --lib gba::integration_tests::gba_suite_tests::
```

These execute:

- `roms/gba/automated_tests/gba-tests/arm/arm.gba`
- `roms/gba/automated_tests/gba-tests/thumb/thumb.gba`
- `roms/gba/automated_tests/gba-tests/memory/memory.gba`

For source-level architecture details, see [architecture.md](architecture.md).
