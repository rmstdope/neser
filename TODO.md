# TODO

## Next priorities (post-Blargg green)

### Pending

- [ ] Build a PPU “pixel hash” regression harness.
  - What: Run the emulator for a fixed number of frames on a small, curated ROM set, then hash the final framebuffer (and/or per-frame hashes).
  - Why: Catch rendering regressions that don’t show up in CPU-focused tests (timing, masking, sprite evaluation quirks, palette issues).
  - Done when: We have a stable ROM list + deterministic output (same host/CI), golden hashes checked in, and a single failing hash clearly points to “render changed”.

- [ ] Implement MMC5 next-tier features (mirroring/ExRAM/IRQ/CHR banking).
  - What: Implement MMC5 in a deliberate order: mirroring via `$5105`, ExRAM mapping at `$5C00-$5FFF` (including mode behaviors), scanline IRQ via `$5203/$5204`, then CHR banking and split-screen behavior.
  - Why: MMC5 enables a bunch of real games/demos and is a common source of subtle PPU/memory interactions.
  - Done when: `roms/blargg/mmc5test*` (and any existing MMC5 test ROMs in this repo) pass the relevant sections and a small MMC5 commercial ROM boots/rendering is plausible.

- [ ] Add CI-friendly test profiling (“fast” vs “slow” test groups).
  - What: Split long-running ROM integration tests into an explicit slow group (e.g. ignored tests or a feature flag) while keeping a fast default suite.
  - Why: Keep local iteration and CI signal fast, while still retaining broad regression coverage.
  - Done when: `cargo test` runs the fast suite quickly, and there’s an obvious opt-in path to run slow tests (e.g. `cargo test -- --ignored` or `cargo test --features slow-tests`).

- [ ] Improve APU correctness beyond reset/mixer.
  - What: Target known APU correctness gaps: frame counter edge cases (4-step/5-step behavior), DMC sample fetch timing + IRQ behavior, and channel enable timing semantics.
  - Why: Many games “mostly work” with approximate APU, but small timing/IRQ differences break audio and sometimes gameplay.
  - Done when: Relevant Blargg APU ROMs in `roms/blargg/` pass (or regressions are reduced), and an “audio signature” regression test exists (e.g. checksum of N frames of mixed samples under fixed conditions).

- [ ] Improve mapper correctness for common commercial titles.
  - What: Focus on high-impact mapper behaviors that affect many ROMs: MMC1 shift-register quirks and reset behavior, MMC3 IRQ/A12 filtering details, and PRG-RAM persistence/enable semantics.
  - Why: Mapper edge cases are a top source of “boots but glitches/crashes later” failures in real games.
  - Done when: A representative set of mapper test ROMs improve/turn green and at least a couple of previously-problematic commercial ROMs behave correctly (boot + stable gameplay past intro).

- [ ] Add a dedicated “DMA torture” regression set.
  - What: Add and run a curated set of ROM tests that stress DMA interactions: OAM DMA on odd/even cycles, CPU dummy reads/writes during DMA, DMC DMA overlaps, and cycle stealing effects on CPU instruction timing.
  - Why: DMA timing bugs often only appear in very specific scenarios and can be hard to debug without targeted regressions.
  - Done when: The torture ROM set runs in CI (likely as slow tests), and failures clearly identify which DMA scenario regressed.

- [ ] Implement save-state support (CPU/PPU/APU + mapper state + RAM).
  - What: Serialize and restore full emulator state, including CPU registers/internal latches, PPU state (including internal counters/buffers), APU state, mapper registers, and RAM/VRAM.
  - Why: Faster debugging (bisect issues), reproducible bug reports, and enables automated “resume from known point” tests.
  - Done when: Loading a state returns to identical execution (deterministic within the same build), state format is versioned, and at least one regression test uses save-states to validate a previously flaky scenario.

### Completed

- [x] Add an automated `nestest.nes` “golden trace” test (CPU regs + PC + flags per instruction) to catch subtle CPU regressions.
- [x] Expand Blargg PPU coverage by wiring in more ROMs from `blargg_ppu_tests_2005.09.15b/` and additional `ppu_vbl_nmi` variants.
- [x] Create a compatibility matrix for `roms/games/*` and add one smoke test per game (boot to title + basic input sanity), logging a short failure signature.
