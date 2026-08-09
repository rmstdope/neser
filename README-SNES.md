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

- `snes-hardware` defaults to auto-detect from the cartridge header country (PAL territories map to `snes-pal`, otherwise `snes-ntsc`); see [PAL and video region](#pal-and-video-region) for the country-code table and what the two modes change.
- `snes-spc-ipl-path` must point to a 64-byte file; invalid files are ignored and NESER falls back to the built-in clean-room IPL ROM.
- `multitap` on port 1 currently falls back to `standard`.

The generic (unprefixed) `ram_init_mode` / `--ram-init-mode` setting also
applies to the SNES since #3128. It controls the power-on contents of WRAM,
VRAM, CGRAM, OAM, APU ARAM and SA-1 I-RAM — `random` (the desktop default,
matching Mesen2's `RamState::Random` and ares), `zero`, or
`seeded-random:SEED` for a randomised but reproducible machine. Battery-backed
save RAM is never touched; it is restored from the `.sav` file. Use `zero` when
comparing captures against another emulator — see
[Baseline (golden) approval workflow](#baseline-golden-approval-workflow).

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
  first-sprite rotation, plus OBJ vertical wrap-around. All 14 goldens
  approved; 13 are Mesen2 cross-checks. `obj-y-wrap.sfc` is the exception:
  Mesen2 disagrees with ares, ares-performance, higan and Snes9x on V-flip
  across a Y wrap (#3003), so its golden rests on that four-implementation
  majority plus the SNESdev wiki, and the suite additionally carries a
  **golden-independent structural assertion** that derives the expected
  wrapped rows from the ROM's own output (`RunResult::screen_rgb` exposes
  the sampled frame for that). The residual Mesen2 diff is confined to the
  flipped band with zero pixels outside it.
- `undisbeliever_ppu_window_tests.rs` -- window mask and INIDISP fade demo
  ROMs built from the undisbeliever source mirror
  (`roms/snes/automated_tests/snes_test_roms/undisbeliever-ppu-window/`, see
  its README): the interactive `window-mask-logic.sfc` (all 21 colour-window
  mask-logic/invert/enable states via input scripting) and
  `window-shapes-single.sfc` (14 HDMA window shapes, locked against its
  auto-advance with an A tap only the initial any-button check reads), the
  two free-running precalculated bouncing-window demos, and
  `inidisp_fadein_fadeout.sfc` sampled mid-plateau at 7 probed frames. All
  44 vectors carry Mesen2-approved 0-px goldens since #3011 fixed the window
  enable/invert decode; before that only the 7 fade samples and the
  no-window state matched. When re-approving the 14 shape goldens, first
  prove the scripted replay lands on the same shape index in both emulators
  by matching each NESER capture against all 14 Mesen2 shape captures -- a
  misaligned index compares two different pictures and looks exactly like a
  rendering bug.
- `neser_color_math_tests.rs` -- NESER-authored colour-math, window and
  brightness ROMs written against the undisbeliever bass framework
  (`roms/snes/automated_tests/snes_test_roms/neser-colormath-tests/`,
  sources included; see its README). One shared Mode 1 quadrant scene (main
  vertical bars x sub horizontal bars = 64 math crossings plus fallback
  regions) covers CGADSUB add/subtract with/without halving, per-plane
  COLDATA fixed-colour math, the transparent-sub fallback rule, the OBJ
  palette 4-7 rule, colour-window clip/prevent and layer window masks;
  `brightness-steps.sfc` steps INIDISP through all 16 levels plus the
  force-blank cut (17 samples). All 27 vectors carry Mesen2-approved 0-px
  goldens since #3012 (transparent-sub halve suppression) and #3011 (window
  decode plus the CGWSEL prevent regions).
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
  All 23 vectors carry Mesen2-approved goldens and none is `#[ignore]`d:
  the pseudo-hires demos were approved with #3016, MosaicMode5 and the
  Interlace suite with #3017, Perspective and InterlaceSimpsonsHDMA with
  #3020, and the StarWars crawl frames with #3021/#3050. The four
  pseudo-hires goldens were re-approved at the native 512x448 with #3034.
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
  All four combos -- default colorbars, graybars, the interlace (X) toggle
  and the 239-line overscan (Y) toggle -- carry Mesen2-approved goldens and
  are un-ignored (#3092), now that the capture-geometry mismatch they were
  parked on is gone (#3001/#3034). The overscan combo's golden CRC is
  byte-identical to colorbars' -- #3001's crop keeps a fixed 224-row
  window but shifts its starting row by 7 when overscan is on, and a
  vertically-uniform pattern like hdrvtest's colorbars can't show that
  shift, so neither can a matching Mesen2 capture (Mesen2 hits the same
  crop). The test therefore
  also asserts `RunResult::overscan_239_enabled` directly, reading the PPU's
  SETINI bit at the sample frame instead of relying on the CRC (#3096).
- `neser_opt_tests.rs` -- NESER-authored offset-per-tile ROMs for BG modes
  2/4/6 (`roms/snes/automated_tests/snes_test_roms/neser-opt-tests/`,
  sources included; see its README) -- no redistributable third-party OPT
  ROM exists. Cover V/H offsets with per-entry BG1/BG2 apply-flag gating,
  the OPT-exempt leftmost column and entry-to-column+1 mapping, the
  ignored low 3 bits of horizontal entries with BG1HOFS fine scroll
  retained, and mode 4's single offset row with bit-15 H/V selection. All
  six carry Mesen2-approved goldens; `opt-m6.sfc` joined them with the
  #3016 hires rework and was re-approved at the native 512x448 with #3034.
- `neser_hires_tests.rs` -- NESER-authored mid-frame hires-transition ROMs
  (`roms/snes/automated_tests/snes_test_roms/neser-hires-tests/`, sources
  included; see its README) -- no vendored ROM switches hires part-way
  down a frame. One HDMA channel turns a hires mode on at display line
  100: `hires-hdma-bgmode.sfc` writes BGMODE 1 -> 5, `hires-hdma-setini.sfc`
  writes SETINI bit 3 (pseudo-hires). Both match Mesen2 pixel-exactly
  (0 of 229,376 px) and additionally assert the structure the golden
  stands for -- rows above the switch column-doubled, rows below carrying
  half-pixel pairs, every row pair identical (#3034).
- `neser_mode7_tests.rs` -- NESER-authored static-matrix Mode 7 ROMs
  (`roms/snes/automated_tests/snes_test_roms/neser-mode7-tests/`, sources
  included; see its README): identity baseline, M7SEL out-of-screen
  wrap / colour 0 / tile 0 fill at an 8x zoom-out, a 30-degree rotation
  about the map centre, both M7SEL screen flips and Mode 7 mosaic. All 8
  match Mesen2 pixel-exactly with approved goldens.
- `hblank_dma_vram_tests.rs` -- HDMA-to-VRAM timing ROMs
  (`roms/snes/automated_tests/snes_test_roms/93143-hblank-dma-vram/`).
- `neser_dma_tests.rs` / `neser_hdma_tests.rs` -- NESER-authored custom
  DMA/HDMA fixtures (#2884), assembled in-code via the shared `fixture_rom.rs`
  builder (no on-disk assets), authored against the fullsnes register spec
  without reading the DMA implementation. GPDMA: mode 0/1 to WRAM/VRAM,
  palette to CGRAM, words to OAM, A-bus increment/decrement/fixed, byte-count
  0 == 65536, ascending multi-channel priority, and #2944 active-display write
  gating (VRAM/CGRAM/OAM readback instruments each proven by a CPU
  write/read self-check). HDMA (targeting WMDATA so per-scanline transfers
  land in readable WRAM): direct-mode line counter, non-repeat idle,
  repeat-every-line, indirect pointer deref, and terminator. Results via the
  WRAM pass/fail marker. B->A direction (#3061): WMDATA `$2180` into cartridge
  SRAM, and the VRAM read port `$2139`/`$213A` into WRAM including its
  prefetch/increment side effects.
- `cartridge_fixtures.rs` / `neser_cartridge_tests.rs` -- NESER-authored base
  cartridge fixtures (#2885), built in-code from the SNES header spec (no
  on-disk assets). Minimal LoROM/HiROM/ExHiROM and copier-header images are
  loaded through the real cartridge/bus paths to verify mapping detection,
  title/country metadata, SRAM size + battery flags, ROM address translation
  (a sentinel byte read back at the mapped CPU address per mapping's
  banks/halves), and 512-byte copier-header stripping. An executable LoROM
  battery-SRAM fixture writes two distinct SRAM addresses and reads one back
  with a CPU self-check (defeating open-bus/MDR false passes). No-enhancement-
  chip carts only; results via the WRAM pass/fail marker plus direct
  cartridge/debugger assertions. Header-field parsing and HiROM/ExHiROM `.sav`
  persistence coverage lives in the `src/snes/cartridge` and
  `src/snes/console/snes.rs` unit tests.
- `jonasquinn_dma_tests.rs` -- canonical DMA/HDMA ROMs from the jonasquinn
  collection (#2884). `test_mdrhdma2/test_mdrhdma.sfc` (MDR-during-HDMA) and
  `hdma_midframe/demo.smc` (mid-frame HDMA visual, also matches the bundled
  `image001.bmp`) have 0-pixel-diff Mesen2 goldens. `test_dmavalid` (#3111),
  `test_hdmadisable` (#3116) and the `test_hdma/test_hdmasync.smc`/
  `test_hdmatiming.smc` byuu mirrors (#3062, #3120) all pass;
  `test_dmatiming` became a 0-px Mesen2 golden in #3127, which also
  un-ignored `test_dmatiming_latches_hv_after_gpdma`.
- `kungfufurby_hdma_tests.rs` -- KungFuFurby HDMA ROMs (#2884,
  `test_hdma`/`test_hdmasync`/`test_hdmatiming`). All three assert the
  Mesen2-correct blue PASS screen (#3062, #3116, #3120).
  `test_hdmatiming_hdma_during_dma_rows_match_mesen2` additionally reads
  rows 9-12 -- the "HDMA during DMA" measurements the ROM records but never
  compares -- out of SRAM; it is the only vector covering HDMA nested inside
  a general-purpose transfer (#3127).
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
- `input_super_scope_tests.rs` -- Super Scope fixtures (#2890), assembled
  in-code via the shared `fixture_rom.rs` builder (no on-disk assets) and
  cross-checked against Mesen2: the 16-bit packet bit layout, fire/pause
  single-shot vs cursor level, turbo toggle plus auto-fire, the offscreen/Null
  bit tracking on/off-screen aim, fire reported off-screen (shoot-to-reload),
  the issue's scripted example sequence, port-1/port-2 routing
  (`$4016`/`$4017`), and the aim latch reading back OPHCT/OPVCT
  (`$213C`/`$213D`) with the Mesen2 centering offset (aimX+10, aimY-3).
  Results via the WRAM pass/fail marker.
- `input_multitap_tests.rs` -- Super Multitap fixtures (#2891), assembled
  in-code via the shared `fixture_rom.rs` builder (no on-disk assets) and
  cross-checked against Mesen2: the strobe-high detection signature
  (`$4017` reads `0x02` while OUT0 is high), a no-buttons baseline and per-slot
  isolation across all four slots, the WRIO `$4201` bit-7 pair select with
  data1/data2 controller ordering, the issue's scripted example sequence
  (slot 1 A, slot 2 B, slot 3 Start, slot 4 Right, then released) with
  held/released transitions, and the auto-joypad path latching the selected
  pair into JOY2/JOY4. Results via the WRAM pass/fail marker.
- `neser_pal_tests.rs` -- PAL timing and video-region fixtures (#2888),
  assembled in-code via the shared `fixture_rom.rs` builder (no on-disk
  assets), authored from fullsnes and cross-checked against ares and Mesen2.
  See [PAL and video region](#pal-and-video-region) below.
- `kungfufurby_nmi_tests.rs` / `kungfufurby_irq_tests.rs` -- KungFuFurby's
  2005-2008 NMI/H-V-IRQ test ROM collection (#2883/#3049,
  `roms/snes/automated_tests/snes_test_roms/KungFuFurby-test-ROMs/`, see its
  README). All nine automated ROMs are green, with nothing `#[ignore]`d:
  `demo_nmitest.smc`, `nmi.smc` and `demo_irqtest.smc` after #3049's
  per-CPU-cycle dispatch fixes (`Cpu::step()` checks interrupt-pending state
  at per-cycle rather than per-instruction granularity, mirroring Mesen2's
  `DetectNmiSignalEdge`/`PrevIrqSource`), `test_nmi.smc` and
  `test_irq4209.smc` after #3116 made STP halt the CPU plus #3081's
  `SetNmiFlag(2)` enable-mid-vblank arm (test_nmi's v1.1 test 27 is its
  hardware witness), `test_irq4200.smc` after #3144 ported Mesen2's
  level+edge IRQ counter circuit (`src/snes/ppu/irq.rs`), `test_irq.smc` and
  `irq.smc` after #3146 moved the IRQ dispatch sample to the start of the CPU
  cycle, and `test_irqb.smc` after #3147. All are backed by SRAM-verdict tests
  transcribed from the vendored byuu sources.

  #3147 is worth knowing about for two reasons. Its headline symptom -- an
  OPHCT latch four dots low in sub-test 4 -- had already been fixed by #3146
  and only needed re-measuring, so the issue's premise was stale before work
  started. What actually remained was sub-test 5: `jmp $217F` executes out of
  the APU comm-port mirrors at `$2144-$217F`, which `SnesSystemBus` decoded as
  open bus rather than as ports (fullsnes: "Ports 2144h..217Fh are APU
  mirrors, NOT open bus"), so the CPU ran a six-cycle `AND (dp,X)` where
  hardware runs the `CLC` this ROM's SPC preamble plants in every port.
- `sour_dma_irq_tests.rs` -- Sour/SnesTests' `dma_irq_test.sfc` (#2883/#3049,
  `roms/snes/automated_tests/snes_test_roms/Sour/SnesTests/`), rebuilt
  byte-identical from source to recover its WRAM result-table address from
  debug symbols. Validates how many instructions run after a manual DMA
  (`$420B` write) before a pending IRQ/NMI dispatches, across 19
  sub-cases. The upstream README's expected-results table has a
  transcription error (`$FFFF` where the real, Mesen2-confirmed sentinel
  is `$00FF`, since the captured value is a single WRAM byte); the golden
  CRC reflects the Mesen2-verified screen. Both tests are active:
  originally 8/19 sub-cases diverged (each off by exactly one fewer
  dispatched instruction than Mesen2, the same signature as the
  KungFuFurby suites); #3049's per-cycle dispatch fixes closed 2 of those
  8 (both `CLI+INC` sub-cases) and #3065 closed the remaining 6. The
  mechanism was later simplified by #3074 (Mesen2's per-cycle DMA
  interrupt lock) and #3081 (instruction-granular `irq_lock_step`
  deleted) without moving any of the 19 values.
- `dsp_audio_golden_tests.rs` -- S-DSP audio sample golden checks: eight
  deterministic 32 kHz capture windows over synthetic in-code BRR fixtures
  (no ROM assets) covering BRR decode, ADSR, GAIN modes, pitch modulation,
  echo/FIR, gaussian interpolation, multi-voice mixing/clamping, and the
  noise LFSR. Each window's baseline is an approved CRC32 plus metadata
  (sample rate, warmup/window lengths, fixture source, review note) inline
  in the test source.

### PAL and video region

The active region is resolved once at ROM load: the `snes-hardware` override
wins if set, otherwise it is auto-detected from the cartridge header's country
code at `$FFD9`. Following fullsnes "Country (also implies PAL/NTSC)", `02h`
through `0Ch` and `11h` (Australia) select PAL; everything else — including
`10h` Brazil, whose PAL-M is a 60 Hz variant, and the codes fullsnes marks
"(?)" — selects NTSC. A ROM can read the active region back at runtime from
STAT78 `$213F` bit 4 (0 = NTSC/60 Hz, 1 = PAL/50 Hz).

What actually differs between the two consoles:

| Behaviour | NTSC | PAL |
| --- | --- | --- |
| Scanlines per frame | 262 (263 interlaced) | 312 (313 interlaced) |
| Last scanline index | 261 | 311 |
| Master clock | 21,477,270 Hz | 21,281,370 Hz |
| Refresh rate | ~60.099 Hz | ~50.007 Hz |
| STAT78 `$213F` bit 4 | 0 | 1 |
| First VBlank scanline | 225 | 225 |
| …with SETINI overscan | 240 | 240 |
| Output dimensions | 256x224, or 512x448 in hi-res/interlace | same |
| SPC700 clock | ~1.025 MHz | ~1.025 MHz |

PAL's extra 50 scanlines are therefore *all* blanking: the active area, the
VBlank boundary and the framebuffer are region-independent, and only the
frame's total length changes. Because the SPC700 runs off its own 24.576 MHz
crystal while the 65816's master clock drops, the SPC runs ~0.92% fast
relative to the CPU on PAL — so the APU derives its clock ratio and audio
sample pacing from the active region too.

`neser_pal_tests.rs` verifies all of the above from fixture ROMs: STAT78
readback per country code and both override directions, V-IRQ existence
probes for the 262/312 scanline counts, an OPVCT latch on the VBlank rising
edge, an SPC700 counter uploaded through the real IPL handshake to measure
the 1.201808 PAL/NTSC time ratio, and cross-region screen comparisons. It
commits no visual baselines — for rendering, the NTSC and PAL runs are each
other's oracle. Region selection in the shared runner is
`RunConfig::with_hardware`; header-driven detection is `FixtureRom::country`.

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
3. If the ROM can display uninitialised RAM, add
   `--snes.RamPowerOnState=AllZeros` to the Mesen2 command line. Mesen2's SNES
   default is `RamState::Random`, and without the flag the ground truth is not
   even self-consistent: for `test_dmatiming/demo.smc` two default Mesen2
   captures differ from *each other* by 1.06%, which is what produced #3063's
   phantom "0.93% DMA-timing divergence". A capture that changes between
   identical runs is the tell.

   Since #3128 NESER's SNES core honours the same `ram_init_mode` setting the
   NES core uses (WRAM, VRAM, CGRAM, OAM, ARAM, SA-1 I-RAM), and its desktop
   default is `random` too — so the *NESER* side needs pinning as well. The
   automated suites already do it: `RunConfig` defaults to
   `RamInitMode::Zero` (`rom_runner.rs`), and `--headless` capture forces
   `zero` and rejects any other value. If you capture by some other route,
   pass `--ram-init-mode zero` explicitly. `AllZeros` on one side and `random`
   on the other measures the RNG, not the emulator.
4. Pixel-diff the two captures programmatically. Never approve a golden from
   a visual comparison alone — eyeballing has repeatedly missed real
   divergences.

   ```bash
   python -m scripts.diff_screenshots neser.png mesen.png --shift-search 1
   ```

   The tool exits non-zero unless the two images are pixel-identical. Since
   the BG vertical-scroll display-line fix (#2945/#2981) the two emulators
   align byte-for-byte at zero offset, so **a non-zero best shift is evidence
   of a bug, never a capture convention to allow for** — the exit code stays
   non-zero even when some offset lines the two up. Add `--rows` for ROMs that
   band by scanline (HDMA writing `$2100` per line): it prints each image's
   per-row mean-luminance vector and the lag that best aligns them, which is
   where a banding-phase error shows up and where a 2D shift search cannot.
5. Only after a 0-pixel diff (or a navigator-reviewed, documented
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
