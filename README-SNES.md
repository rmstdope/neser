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

SNES integration tests live under `src/snes/integration_tests/`. Run them
during development with:

```bash
./scripts/test-dir.sh src/snes
```

Test suites:

- `processor_tests_65816.rs` / `processor_tests_spc700.rs` -- single-step
  CPU/SPC-700 vector tests, with an optional local full-corpus cache under
  `roms/snes/automated_tests/processor_tests/*/full/v1`.
- `blargg_apu_tests.rs` -- 18 SPC700/APU test ROMs
  (`roms/snes/automated_tests/blargg_apu/`).
- `gilyon_cpu_tests.rs` / `gilyon_spc_tests.rs` -- 65816 and SPC-700 CPU test
  ROMs (`roms/snes/automated_tests/snes_test_roms/gilyon/`).
- `peterlemon_cpu_tests.rs` / `peterlemon_spc_tests.rs` -- PeterLemon (krom)
  per-opcode-group 65816 and SPC-700 test ROMs
  (`roms/snes/automated_tests/snes_test_roms/PeterLemon/SNES-CPUTest-*/`).
- `peterlemon_ppu_bg_tests.rs` -- PeterLemon basic PPU BG demos: 2/4/8bpp
  tile decoding, BG1-BG4 tilemaps, all four tilemap screen sizes, tile
  flip, backdrop and palettes
  (`roms/snes/automated_tests/snes_test_roms/PeterLemon/SNES-PPU-*/`).
  11 of 12 automated; `8x8BGMap8BPP32x32.sfc` is excluded pending the
  animation frame-phase divergence tracked in #2990.
- `undisbeliever_tests.rs` -- hardware-glitch/timing-hammer ROMs
  (`roms/snes/automated_tests/snes_test_roms/undisbeliever-inidisp/`).
- `undisbeliever_ppu_bg_tests.rs` -- VMAIN/VRAM-increment and basic BG ROMs
  built from the undisbeliever source mirror
  (`roms/snes/automated_tests/snes_test_roms/undisbeliever-ppu-bg/`, see its
  README for the build procedure). 8 of 18 automated; the 6
  `*-with-remapping` ROMs are excluded pending VMAIN address remapping
  (#2989) and the 3 scrolling ROMs plus `textbuffer-hello-world.sfc` pending
  the animation frame-phase divergence (#2990).
- `hblank_dma_vram_tests.rs` -- HDMA-to-VRAM timing ROMs
  (`roms/snes/automated_tests/snes_test_roms/93143-hblank-dma-vram/`).
- `sa1_absindx_tests.rs` -- absindx SA-1 conformance ROMs
  (`roms/snes/automated_tests/snes_test_roms/absindx/`), verified with
  human-approved screen-CRC goldens. `SA1RamProtectionTest.sfc` passes all 222 sub-tests
  (golden shows `Result Passed`); `SA1VersionCodeTest.sfc`'s golden captures
  the hardware-accurate register-dump screen, whose result line reads
  `Failed` even on real hardware (its pass path is unreferenced dead code,
  verified by disassembly -- deliberate, since the SA-1's true version-code
  value is unknown and `$230E` is open bus).
- `sa1_boot_tests.rs` / `sa1_iram_tests.rs` / `sa1_bwram_tests.rs` /
  `sa1_irq_tests.rs` -- hand-assembled SA-1 fixture-ROM tests for dual-CPU
  boot, shared I-RAM/BW-RAM exchange, and the cross-CPU IRQ handshake.
- `dsp_audio_golden_tests.rs` -- S-DSP audio sample golden checks: eight
  deterministic 32 kHz capture windows over synthetic in-code BRR fixtures
  (no ROM assets) covering BRR decode, ADSR, GAIN modes, pitch modulation,
  echo/FIR, gaussian interpolation, multi-voice mixing/clamping, and the
  noise LFSR. Each window's baseline is an approved CRC32 plus metadata
  (sample rate, warmup/window lengths, fixture source, review note) inline
  in the test source.

Most ROM-based suites report pass/fail either through a text shell
(blargg/gilyon) or by rendering a known-good screen; `rom_runner.rs` provides
the shared headless runner (tick/frame budgets, WRAM-marker and bus-byte
oracles) and a screen-CRC oracle that runs to a fixed frame and compares the
rendered screen CRC32 against an approved golden. Set
`NESER_CAPTURE_SCREEN=1` to write a PNG per test under
`target/snes_test_captures/<suite>/` when approving a new golden; each
suite's own source file documents how to record the result.

Audio goldens follow the same approval workflow with
`NESER_CAPTURE_AUDIO=1`, which writes a 16-bit stereo 32 kHz WAV per test
under `target/snes_test_captures/dsp_audio_golden_tests/`. Review the WAV by
listening and/or plotting it (`python scripts/display_audio_output.py
<wav>`), then record the printed CRC and a review note in the test's
`GoldenAudioWindow` metadata. Capture artifacts live under the git-ignored
`target/` directory and are never committed.

Asset provenance (source URL/ref, license, oracle type) is tracked in
`roms/snes/automated_tests/manifest.json` and validated by
`python -m scripts.validate_snes_test_assets`. Intake policy and
baseline-approval rules are documented in
[docs/SNES_TEST_ASSET_POLICY.md](docs/SNES_TEST_ASSET_POLICY.md).
