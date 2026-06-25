# Super Nintendo (SNES) support in NESER

This file covers SNES-specific usage for native and web frontends. For installation, generic build/run commands, config file locations, and development setup, see [README.md](README.md).

## Running SNES ROMs (native frontend)

NESER auto-detects SNES ROMs by extension (`.sfc` / `.smc`):

```bash
neser path/to/game.sfc
neser path/to/game.smc
cargo run --release --bin neser -- path/to/game.sfc
```

Use `neser --help` for the complete current CLI reference.

## SNES configuration (native frontend)

SNES-specific options:

- `--snes-hardware <snes-ntsc|snes-pal>`
- `--snes-spc-ipl-path <path>`
- `--snes-controller-port1 <standard|multitap|mouse|superscope>`
- `--snes-controller-port2 <standard|multitap|mouse|superscope>`

Examples:

```bash
neser --snes-hardware snes-pal path/to/game.sfc
neser --snes-controller-port1 mouse --snes-controller-port2 standard path/to/game.sfc
```

Equivalent config keys in `neser.conf`:

```text
#snes-hardware=snes-ntsc
#snes-spc-ipl-path=/absolute/path/to/spc_ipl.bin
#snes-controller-port1=standard
#snes-controller-port2=standard
```

Notes:

- `snes-hardware` defaults to auto-detect from the cartridge header country (PAL territories map to `snes-pal`, otherwise `snes-ntsc`).
- `snes-spc-ipl-path` must point to a 64-byte file; invalid files are ignored and NESER falls back to the built-in clean-room IPL ROM.
- `multitap` on port 1 currently falls back to `standard`.

## SNES input

Default keyboard mapping (native and web SNES frontends):

| SNES button | Keyboard |
| --- | --- |
| D-pad | `W`/`A`/`S`/`D` (native also supports arrow keys) |
| B | `R` |
| A | `T` |
| X | `Y` |
| Y | `G` |
| L | `Q` |
| R | `E` |
| Select | `4` |
| Start | `5` |

The native frontend also supports gamepads through `gilrs`.

## SNES in the web frontend

The browser frontend accepts SNES ROM uploads with `.sfc` and `.smc` extensions.

SNES web behavior:

- Uses the dedicated WASM SNES runtime.
- Supports pause/resume, soft/hard reset, audio playback, and save/load state in browser storage.
- Uses the stock filter only for SNES (`F4` does not cycle through NES/GB shader sets in SNES mode).

For browser build/run/test commands, see [web/README.md](web/README.md).

## SNES automated verification

SNES integration tests live under `src/snes/integration_tests/`.

- `processor_tests_65816.rs` and `processor_tests_spc700.rs` run pinned
  Tom Harte ProcessorTests vector subsets, with optional local full-corpus
  caches under `roms/snes/automated_tests/processor_tests/*/full/v1`.
- `scripts/refresh_65816_processor_tests_subset.py` regenerates the committed
  65816 CI subset deterministically from the local full corpus and writes
  `roms/snes/automated_tests/processor_tests/65816/subset_coverage_report.json`
  with selected-opcode family coverage and tree-integrity metadata. By default,
  it also truncates each selected opcode file to the first 32 vectors to keep
  committed CI assets compact.
- `rom_runner.rs` provides the shared headless ROM runner used by future
  ROM-based SNES verification suites. It loads generated or vendored `.sfc` /
  `.smc` bytes through the SNES console, runs with explicit tick/frame budgets,
  detects pass/fail through a reserved WRAM marker at `$7E1FF0`, records
  diagnostics, and computes a screen CRC.
- Asset provenance is tracked in
  `roms/snes/automated_tests/manifest.json` and validated by
  `python -m scripts.validate_snes_test_assets`.
- Intake policy and baseline-approval rules are documented in
  [docs/SNES_TEST_ASSET_POLICY.md](docs/SNES_TEST_ASSET_POLICY.md).
- Set `NESER_CAPTURE_SCREEN=1` to write optional runner screenshots under
  `target/snes_test_captures/`.

Run SNES tests during development with:

```bash
./scripts/test-dir.sh src/snes
```
