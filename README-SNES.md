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
./scripts/test-dir.sh src/snes                    # unit + committed integration subsets
./scripts/test-dir.sh src/snes --skip-integration # unit tests only (fast iteration)
```

`--skip-integration` skips the `snes::integration_tests` module (all
ROM/vector suites below) along with the other consoles' integration modules.
Run the full `src/snes` selection before creating a PR.

CI runs the SNES suites whenever `src/snes/**` or
`roms/snes/automated_tests/**` changes (including bumps of the
`snes_test_roms` submodule pointer); cross-cutting `src/platform` or
crate-root changes run the whole test suite. Only the committed subsets run
in CI — no optional corpus is required.

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
  DRAM-refresh-stall-during-DMA work tracked in #2985 (its RDNMI poll-race
  scroll cadence itself matches Mesen2 since #2990).
- `undisbeliever_tests.rs` -- hardware-glitch/timing-hammer ROMs
  (`roms/snes/automated_tests/snes_test_roms/undisbeliever-inidisp/`).
- `undisbeliever_ppu_bg_tests.rs` -- VMAIN/VRAM-increment and basic BG ROMs
  built from the undisbeliever source mirror
  (`roms/snes/automated_tests/snes_test_roms/undisbeliever-ppu-bg/`, see its
  README for the build procedure). All 18 automated, including the 3
  scrolling ROMs and `textbuffer-hello-world.sfc` whose animated goldens are
  derived from frame-skip-free Mesen2 captures at frame 120 (#2990), and the
  6 `*-with-remapping` ROMs added once VMAIN $2115 bits 2-3 address
  remapping was implemented (#2989; each matches Mesen2 pixel-exactly and,
  by demo design, its no-remapping twin's golden CRC).
- `undisbeliever_ppu_obj_tests.rs` -- the OBJ/sprite-limit dropout ROM built
  from the undisbeliever source mirror
  (`roms/snes/automated_tests/snes_test_roms/undisbeliever-ppu-obj/`).
  Automated with a Mesen2-approved golden since the #2999 OBJ eval/fetch
  pipeline (34-sliver time-over limit) landed.
- `byuu_test_oam_tests.rs` -- byuu's interactive `test_oam.smc` menu
  (`roms/snes/automated_tests/snes_test_roms/jonasquinn-test-roms/test_oam/`)
  driven through `rom_runner`'s frame-stamped input scripting. 30 combos
  (menu, all 8 OBSEL bases x both size bits, flips, char variants, OBJ
  interlace since #3000) carry Mesen2-approved goldens; 4 SETINI combos are
  `#[ignore]`d -- the screen-interlace/overscan capture dimensions differ
  from Mesen2's (#3001).
- `neser_obj_tests.rs` -- NESER-authored OBJ feature ROMs written against
  the undisbeliever bass framework
  (`roms/snes/automated_tests/snes_test_roms/neser-obj-tests/`, sources
  included; see its README): all eight OBSEL size pairs, OBJ palettes,
  OBJ-vs-OBJ priority, OAM X bit 8, mode-1 OBJ-vs-BG layering and OAMADDH
  first-sprite rotation, 13 of 14 with Mesen2-approved goldens.
  `obj-y-wrap.sfc` is `#[ignore]`d pending the V-flip+Y-wrap divergence
  (#3003).
- `undisbeliever_ppu_window_tests.rs` -- window mask and INIDISP fade demo
  ROMs built from the undisbeliever source mirror
  (`roms/snes/automated_tests/snes_test_roms/undisbeliever-ppu-window/`, see
  its README): the interactive `window-mask-logic.sfc` (all 21 colour-window
  mask-logic/invert/enable states via input scripting) and
  `window-shapes-single.sfc` (14 HDMA window shapes, locked against its
  auto-advance with an A tap only the initial any-button check reads), the
  two free-running precalculated bouncing-window demos, and
  `inidisp_fadein_fadeout.sfc` sampled mid-plateau at 7 probed frames. The
  7 fade samples and the no-window state carry Mesen2-approved goldens; all
  36 window-enabled vectors are `#[ignore]`d because NESER renders inverted
  window-masking regions (#3011).
- `neser_color_math_tests.rs` -- NESER-authored colour-math, window and
  brightness ROMs written against the undisbeliever bass framework
  (`roms/snes/automated_tests/snes_test_roms/neser-colormath-tests/`,
  sources included; see its README). One shared Mode 1 quadrant scene (main
  vertical bars x sub horizontal bars = 64 math crossings plus fallback
  regions) covers CGADSUB add/subtract with/without halving, per-plane
  COLDATA fixed-colour math, the transparent-sub fallback rule, the OBJ
  palette 4-7 rule, colour-window clip/prevent and layer window masks;
  `brightness-steps.sfc` steps INIDISP through all 16 levels plus the
  force-blank cut (17 samples). add-clamp, sub-floor, both fixed-colour
  ROMs and all 17 brightness samples carry Mesen2-approved goldens; the
  half-math ROMs are `#[ignore]`d pending the transparent-sub
  halve-suppression rule (#3012) and the two window ROMs pending #3011.
- `jonasquinn_math_tests.rs` -- `color_halve_proof/demo.smc` from the
  jonasquinn collection
  (`roms/snes/automated_tests/snes_test_roms/jonasquinn-test-roms/`),
  proving half colour math halves after the add via per-scanline COLDATA
  rewrites; Mesen2-approved golden. `test_math.sfc` was evaluated and left
  un-automated (a CPU mul/div latency test whose screen CRC would only
  prove the ROM ran; see the manifest notes).
- `peterlemon_ppu_advanced_tests.rs` -- PeterLemon (krom) advanced PPU mode
  demos (#2881) from the four subtree mirrors under
  `roms/snes/automated_tests/snes_test_roms/PeterLemon/SNES-PPU-{Mode7,Mosaic,Interlace,HDMA-HiColor64PerTileRowPseudoHiRes}/`:
  Mode 7 rotozoom (static plus input-scripted rotate/zoom holds), the HDMA
  Perspective and animated StarWars crawl demos, mosaic in modes 3 and 5,
  six true-hires/interlace demos and four pseudo-hires HiColor demos.
  RotZoom (3 vectors), MosaicMode3 (2) and StarWars frame 120 carry
  Mesen2-approved goldens. `#[ignore]`d with NESER's current CRCs:
  Perspective (rightmost-column divergence, #3020), StarWars f360/f600
  (crawl drift, #3021), MosaicMode5 and the six Interlace demos (mode 5/6
  hires column rendering #3016 and line-doubled interlace fields #3017),
  and the four pseudo-hires demos (#3018).
- `undisbeliever_ppu_mode7_tests.rs` -- Mode 7 VRAM-layout and tilemap
  demos built from the undisbeliever source mirror
  (`roms/snes/automated_tests/snes_test_roms/undisbeliever-ppu-mode7/`, see
  its README). The four static `vmain-mode7-image-*` demos (VMAIN
  low-byte-only, tilemap-then-tiles, 8/10-bit remapping) share one
  Mesen2-approved golden by design; the two animated `vmain-mode7-tilemap-*`
  demos carry Mesen2-approved goldens at frames 120/360/600 each. All six
  match Mesen2 pixel-exactly.
- `ddribin_hdrv_tests.rs` -- ddribin's CC0 HDRV display-mode test ROM
  (`roms/snes/automated_tests/snes_test_roms/ddribin-hdrv-snes-test/`,
  built once with WLA-DX; see its README), driven through `rom_runner`
  input scripting (its splash screen ignores input until ~frame 300).
  Default colorbars and the graybars pattern carry Mesen2-approved
  goldens; the interlace (X) and 239-line overscan (Y) combos are
  content-verified against Mesen2 (0-pixel diffs after width-halving resp.
  at crop offset 0) but `#[ignore]`d with NESER CRCs pending the #3001
  capture-geometry convention.
- `neser_opt_tests.rs` -- NESER-authored offset-per-tile ROMs for BG modes
  2/4/6 (`roms/snes/automated_tests/snes_test_roms/neser-opt-tests/`,
  sources included; see its README) -- no redistributable third-party OPT
  ROM exists. Cover V/H offsets with per-entry BG1/BG2 apply-flag gating,
  the OPT-exempt leftmost column and entry-to-column+1 mapping, the
  ignored low 3 bits of horizontal entries with BG1HOFS fine scroll
  retained, and mode 4's single offset row with bit-15 H/V selection. The
  five mode 2/4 ROMs carry Mesen2-approved goldens; `opt-m6.sfc` is
  `#[ignore]`d (mode 6 16x8 tile pairing / hires columns, #3019/#3016).
- `neser_mode7_tests.rs` -- NESER-authored static-matrix Mode 7 ROMs
  (`roms/snes/automated_tests/snes_test_roms/neser-mode7-tests/`, sources
  included; see its README): identity baseline, M7SEL out-of-screen
  wrap / colour 0 / tile 0 fill at an 8x zoom-out, a 30-degree rotation
  about the map centre, both M7SEL screen flips and Mode 7 mosaic. All 8
  match Mesen2 pixel-exactly with approved goldens.
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
- `input_standard_controller_tests.rs` -- standard-controller protocol
  fixtures (#2886), assembled in-code via the shared `fixture_rom.rs`
  builder (no on-disk assets): `$4016`/`$4017` serial order incl. the four
  ID zeros and connected-pad padding ones, the issue's scripted example
  press/release sequence observed through auto-joypad JOY1 reads, strobe-high
  live-B/falling-edge latch semantics, port 2 coverage, and auto-joypad vs
  manual serial layout equivalence. Results are reported through the WRAM
  pass/fail marker; no visual baselines.
- `input_mouse_tests.rs` -- SNES Mouse protocol fixtures (#2889), assembled
  in-code via the shared `fixture_rom.rs` builder (no on-disk assets):
  32-bit packet identification (zero lead byte, hardware ID `0001`, tail
  ones past bit 32), the issue's scripted example sequence (all four motion
  directions with fullsnes sign/direction-bit conventions, left/right/both
  button edges, release), sensitivity cycling 0->1->2->0 on `$4016` clocks
  while the strobe is high, 7-bit magnitude clamping at +/-127, and a
  port-2 mouse on `$4017`. Results via the WRAM pass/fail marker.
- `kungfufurby_nmi_tests.rs` / `kungfufurby_irq_tests.rs` -- KungFuFurby's
  2005-2008 NMI/H-V-IRQ test ROM collection (#2883/#3049,
  `roms/snes/automated_tests/snes_test_roms/KungFuFurby-test-ROMs/`, see its
  README). `demo_nmitest.smc`, `nmi.smc` and `demo_irqtest.smc` match
  Mesen2 exactly and carry approved goldens, after #3049's per-CPU-cycle
  NMI and H/V-IRQ dispatch fixes (`Cpu::step()` now checks
  interrupt-pending state at per-cycle rather than per-instruction
  granularity, mirroring Mesen2's `DetectNmiSignalEdge`/`PrevIrqSource`).
  `test_nmi.smc`, `irq.smc`, `test_irq.smc`, `test_irq4200.smc`,
  `test_irq4209.smc` and `test_irqb.smc` remain `#[ignore]`d: unaffected by
  either fix (identical CRCs before/after), root cause not yet identified.
- `sour_dma_irq_tests.rs` -- Sour/SnesTests' `dma_irq_test.sfc` (#2883/#3049,
  `roms/snes/automated_tests/snes_test_roms/Sour/SnesTests/`), rebuilt
  byte-identical from source to recover its WRAM result-table address from
  debug symbols. Validates how many instructions run after a manual DMA
  (`$420B` write) before a pending IRQ/NMI dispatches, across 19
  sub-cases. The upstream README's expected-results table has a
  transcription error (`$FFFF` where the real, Mesen2-confirmed sentinel
  is `$00FF`, since the captured value is a single WRAM byte); the golden
  CRC reflects the Mesen2-verified screen. `#[ignore]`d pending a #3049
  follow-up: originally 8/19 sub-cases diverged (each off by exactly one
  fewer dispatched instruction than Mesen2, the same signature as the
  KungFuFurby suites); #3049's per-cycle dispatch fixes closed 2 of those
  8 (both `CLI+INC` sub-cases), 6 remain.
- `dsp_audio_golden_tests.rs` -- S-DSP audio sample golden checks: eight
  deterministic 32 kHz capture windows over synthetic in-code BRR fixtures
  (no ROM assets) covering BRR decode, ADSR, GAIN modes, pitch modulation,
  echo/FIR, gaussian interpolation, multi-voice mixing/clamping, and the
  noise LFSR. Each window's baseline is an approved CRC32 plus metadata
  (sample rate, warmup/window lengths, fixture source, review note) inline
  in the test source.

### Optional full ProcessorTests corpora

The ProcessorTests suites run committed vector subsets
(`roms/snes/automated_tests/processor_tests/*/v1`). The full upstream
corpora are intentionally git-ignored, local-only, and never required by
CI. Fetch them with `scripts/refresh_65816_processor_tests_subset.sh` /
`scripts/refresh_spc700_processor_tests_subset.sh`; once files exist under
`.../full/v1`, they transparently override same-named committed subset
files on the next test run. An `#[ignore]`d debug helper additionally runs
an arbitrary external SPC700 vector set via
`NESER_SPC700_EXTERNAL_VECTORS_ROOT`. The subset-regeneration flow and
provenance rules are in
[docs/SNES_TEST_ASSET_POLICY.md](docs/SNES_TEST_ASSET_POLICY.md).

### Baseline (golden) approval workflow

Screen and audio goldens are compact committed CRCs approved from review
artifacts; the artifacts themselves live under the git-ignored `target/`
directory and are never committed. To approve a new or changed golden:

1. Run the test with `NESER_CAPTURE_SCREEN=1` to write a PNG per test under
   `target/snes_test_captures/<suite>/` (each suite's source file documents
   its specific recording steps).
2. Capture the Mesen2 ground truth for the same ROM/frame (headless
   `--testRunner` with a Lua screenshot script). Always pass
   `--Video.VideoFilter=None --Video.AspectRatio=NoStretching
   --snes.disableFrameSkipping=true` — without the frame-skip switch
   Mesen2's testRunner renders only every other frame and screenshots of
   animated content show stale frames (found in #2990); the video overrides
   keep personal Mesen2 config from rescaling or filtering the capture.
3. Pixel-diff the two captures programmatically (e.g. with PIL). Never
   approve a golden from a visual comparison alone — eyeballing has
   repeatedly missed real divergences.
4. Only after a 0-pixel diff (or a navigator-reviewed, documented
   deviation), embed the approved `screen_crc32()` value in the test and
   note the approval basis.

Most ROM-based suites report pass/fail either through a text shell
(blargg/gilyon) or by rendering a known-good screen; `rom_runner.rs` provides
the shared headless runner (tick/frame budgets, WRAM-marker and bus-byte
oracles) and a screen-CRC oracle that runs to a fixed frame and compares the
rendered screen CRC32 against an approved golden. Interactive ROMs can be
driven deterministically with `RunConfig::with_input_script`: a sorted list
of frame-stamped controller-1 button edges applied as the frame counter
advances (see `byuu_test_oam_tests.rs` for the script-builder pattern and
the Mesen2 replay recipe used to approve its goldens). New and changed
goldens are approved through the workflow above.

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
