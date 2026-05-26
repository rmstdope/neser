# Game Boy support in NESER

This file covers Game Boy and Game Boy Color-specific usage. For installation, generic build/run commands, configuration file locations, and development setup, see [README.md](README.md).

## Supported hardware targets

NESER can run Game Boy ROMs in DMG, CGB, or GBA-in-GBC compatibility mode:

```bash
neser --gb-hardware dmg path/to/game.gb
neser --gb-hardware cgb path/to/game.gbc
neser --gb-hardware gba path/to/game.gbc
```

If `gb-hardware` is not set, NESER auto-detects the target from the ROM header:

- DMG-only ROMs run as DMG.
- Dual-compatible and CGB-only ROMs run as CGB.
- CGB-only ROMs cannot be forced into DMG mode.

## Hardware revisions

DMG variants:

- `dmg-0`
- `dmg-a`
- `dmg-b`
- `dmg-c`

CGB variants:

- `cgb-0`
- `cgb-a`
- `cgb-b`
- `cgb-c`
- `cgb-d`
- `cgb-e`

Examples:

```bash
neser --gb-dmg-variant dmg-0 path/to/game.gb
neser --gb-cgb-variant cgb-c --gb-hardware cgb path/to/game.gbc
```

Equivalent config keys:

```text
gb-hardware=cgb
gb-dmg-variant=dmg-b
gb-cgb-variant=cgb-e
```

## Running GB/GBC ROMs

```bash
neser path/to/game.gb
neser path/to/game.gbc
cargo run --release --bin neser -- path/to/game.gb
```

Use `neser --help` for the complete current CLI reference.

## Input

Default keyboard mapping:

| Game Boy button | Keyboard |
| --- | --- |
| D-pad | `W`/`A`/`S`/`D` or arrow keys |
| A | `T` |
| B | `R` |
| Select | `4` |
| Start | `5` |

The native frontend also supports gamepads through `gilrs`.

## Video and filters

Game Boy shader presets are selected with `--gb-filter` or `gb-filter`:

```bash
neser --gb-filter dmg path/to/game.gb
```

Documented presets:

- `dmg`
- `none`

## Boot animation

The Game Boy boot animation can be enabled with:

```text
gb-boot-animation=true
```

The default is to skip the boot animation for faster startup.

## Automated Game Boy tests

GB automated tests live under:

- `src/gb/integration_tests/`
- `roms/gb/automated_tests/`

Major covered suites include:

- GBEmulatorShootout visual and hardware-probe rows
- Blargg CPU, timing, OAM bug, and sound tests
- Mooneye acceptance and emulator-only tests
- SameSuite APU, DMA, PPU, interrupt, and SGB command tests
- Mealybug Tearoom PPU visual tests
- ax6 `rtc3test`
- daid GB/GBC tests
- CasualPokePlayer MBC3/RTC tests

Useful commands:

```bash
./scripts/test-dir.sh src/gb --skip-integration
cargo test --no-default-features --lib gb::integration_tests::
```

At the time this README was written, NESER had automated passing coverage for all current GBEmulatorShootout rows locally. Re-check with:

```bash
cargo test --no-default-features --lib gb::integration_tests -- --nocapture
```

For source-level architecture details, see [architecture.md](architecture.md).
