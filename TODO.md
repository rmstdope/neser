# TODO

## Next priorities (post-Blargg green)

### Pending

- [ ] Improve mapper correctness for common commercial titles.
  - What: Focus on high-impact mapper behaviors that affect many ROMs: MMC1 shift-register quirks and reset behavior, MMC3 IRQ/A12 filtering details, and PRG-RAM persistence/enable semantics.
  - Why: Mapper edge cases are a top source of “boots but glitches/crashes later” failures in real games.
  - Done when: A representative set of mapper test ROMs improve/turn green and at least a couple of previously-problematic commercial ROMs behave correctly (boot + stable gameplay past intro).

- [ ] Implement save-state support (CPU/PPU/APU + mapper state + RAM).
  - What: Serialize and restore full emulator state, including CPU registers/internal latches, PPU state (including internal counters/buffers), APU state, mapper registers, and RAM/VRAM.
  - Why: Faster debugging (bisect issues), reproducible bug reports, and enables automated “resume from known point” tests.
  - Done when: Loading a state returns to identical execution (deterministic within the same build), state format is versioned, and at least one regression test uses save-states to validate a previously flaky scenario.

- [ ] Implement an auto runner for games
  - What: Create a system that can run games automatically, simulating controller input to progress through gameplay without user interaction. Controller input sequences can be recorded by a manual test run and then replayed by the auto runner. This should be a visual test only, without the need for state comparisons.
  - Why: Verification of rendering of commercial titles over extended play sessions, catching graphical glitches or rendering issues that may only appear after prolonged gameplay.
  - Done when: The auto runner can recording input in a manual session and replay it accurately, allowing the game to progress through various stages automatically while visually verifying correct rendering.

### Completed

- [x] Add an automated `nestest.nes` “golden trace” test (CPU regs + PC + flags per instruction) to catch subtle CPU regressions.
- [x] Expand Blargg PPU coverage by wiring in more ROMs from `blargg_ppu_tests_2005.09.15b/` and additional `ppu_vbl_nmi` variants.
- [x] Create a compatibility matrix for `roms/games/*` and add one smoke test per game (boot to title + basic input sanity), logging a short failure signature.
- [x] Implement MMC5 next-tier features (mirroring/ExRAM/IRQ/CHR banking).
