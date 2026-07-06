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
  committed CI assets compact (`--max-vectors-per-file 0` disables truncation).
- `scripts/refresh_spc700_processor_tests_subset.py` regenerates the committed
  SPC700 CI subset deterministically from the local full corpus and writes
  `roms/snes/automated_tests/processor_tests/spc700/subset_coverage_report.json`
  with selected-opcode family coverage and tree-integrity metadata. By default,
  it truncates selected opcode files to the first 32 vectors
  (`--max-vectors-per-file 0` disables truncation).
- `rom_runner.rs` provides the shared headless ROM runner used by future
  ROM-based SNES verification suites. It loads generated or vendored `.sfc` /
  `.smc` bytes through the SNES console, runs with explicit tick/frame budgets,
  detects pass/fail through a reserved WRAM marker at `$7E1FF0`, records
  diagnostics, and computes a screen CRC.
- `rom_pass_fail` suite for #2876 vendors all 18 blargg SNES SPC700/APU test ROMs as
  `snes-rom-pass-fail-blargg-spc-apu`. Seventeen are verified by the `rom_runner`
  screen-CRC oracle (run to a fixed frame, compare the screen CRC32 against a
  human-approved PASS capture). The remaining one is committed with an `#[ignore]`'d
  test pending emulator accuracy improvements.
- Asset provenance is tracked in
  `roms/snes/automated_tests/manifest.json` and validated by
  `python -m scripts.validate_snes_test_assets`.
- For SNES `processor_tests` entries, `source.ref` must be a pinned 40-char
  lowercase commit SHA, and assets sharing a `source.url` must share the same
  pinned ref.
- Intake policy and baseline-approval rules are documented in
  [docs/SNES_TEST_ASSET_POLICY.md](docs/SNES_TEST_ASSET_POLICY.md).
- Set `NESER_CAPTURE_SCREEN=1` to write optional runner screenshots under
  `target/snes_test_captures/`.

### blargg SPC700/APU ROM suite (#2876)

All 18 blargg SNES SPC700/APU test ROMs are committed under
`roms/snes/automated_tests/rom_pass_fail/blargg_spc_apu/v1/`. They report
results through blargg's text shell, so each ROM is verified by the `rom_runner`
**screen-CRC oracle**: run to a fixed frame, then compare the rendered screen
CRC32 against a human-approved PASS capture. To approve a new golden, run with
`NESER_CAPTURE_SCREEN=1`, visually confirm the capture under
`target/snes_test_captures/` shows a PASS screen, record the `(frame, CRC)` in
`src/snes/integration_tests/rom_pass_fail_spc700.rs`, and recompute the
committed integrity with
`python -m scripts.compute_snes_rom_asset_integrity <dir>`.

**Passing (18) — visually-approved screen-CRC goldens:**

| ROM | Category | Golden CRC |
| --- | --- | --- |
| `1-test_exec_from_io` | SMP | `0x7EEE5E15` |
| `2-test_single_instr` | SMP | `0x2B42CE76` |
| `3-test_write_disable` | SMP | `0xC3DE3F4F` |
| `4-test_ram_disable` | SMP | `0x85F1D154` |
| `test_ram_disable_ipl` | SMP | `0xD001765E` |
| `spc_smp` | SMP | `0xEFD13576` (frame 2200) |
| `spc_mem_access_times` | SMP | `0x3AC3E30F` |
| `spc_dsp6` | DSP | `0x05CD5DA7` (frame 9100, see below) |
| `spc_timer` | Timers | `0x249738B2` |
| `test_speed` | Timers | `0xFAE499DA` |
| `test_timer_speed` | Timers | `0xA4D0ACB0` |
| `test_timer_speed2` | Timers | `0xA4D0ACB0` |
| `test_timer_speed_2` | Timers | `0xCAF1E3BC` |
| `test_timer_speed3` | Timers | `0x367A08A5` |
| `test_timer_stop` | Timers | `0x7CC2B76B` |
| `test_timer_stop2` | Timers | `0xB2CC2986` |
| `speed_2_freezes2` | Timers | `0x6E1BF905` |
| `timer_at_power_reset` | Timers | `0x9A3B5FC3` (mid-test reset, see below) |

`test_timer_speed3` is a measurement-oriented ROM: its golden is a visually
approved `Done` screen rather than an internal `Passed` text screen.

`timer_at_power_reset` signals that it wants a physical reset partway through
by jumping into zeroed-out low WRAM (`$0000`), which real test hardware
answers with a reset-button press. `RunConfig::with_reset_on_pc_trap(0x0000)`
models this handshake: the runner triggers a soft CPU reset every time PC
reaches the trap address (capped at 16 auto-resets) so the ROM can continue
into its post-reset comparison and reach its real Passed/Failed screen.

`test_timer_stop2` exercises the TEST-register ($F0) global timer stop: it
rapidly pumps the stop/start bits and expects each stop issued while the
timer's stage-1 prescaler square wave is high to inject one target-counter
clock (the forced-low input is a falling edge at the timer's edge detector),
with TnOUT preserved across the stop.

`spc_dsp6` runs blargg's full S-DSP suite (KON, Misc, Order, Random and
Timing batteries, ~9000 frames) and ends with a "PASSED TESTS" screen on a
blue background; its test uses a 600M-tick budget to reach frame 9100. Note
that real 3-chip SNES consoles fail parts of this ROM — the golden matches
Mesen2's DSP model, which passes it fully.

Run SNES tests during development with:

```bash
./scripts/test-dir.sh src/snes
```
