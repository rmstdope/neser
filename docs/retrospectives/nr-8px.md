# nr-8px — retrospective

- **Implementer:** Nightcrawler
- **Date:** 2026-10-04
- **PR:** #3341

## Two read_joy3 tests pinned NESER's own wrong output and hid a bug

**What happened.** `test_read_joy3_count_errors` and `_fast` expected "CONFLICTS: 0/1000" and "ERRORS: 0/1000". Mesen2 shows 67/1000 and 15/1000 on the same ROMs: a DMC fetch that halts a `$4016` read deletes a bit on hardware (NESdev "DMA", Register conflicts). NESER never deleted one, and the tests had locked that in. The bug turned up only when Ninja Gaiden's DPCM-safe double read took a different branch from Mesen2's in an exec trace.
**Why.** The expected strings were taken from NESER's own screen. These ROMs print a count and have no pass/fail of their own, so any count passed review.
**Cost.** About an hour tracing Ninja Gaiden before the cause was found.
**Prevent by.** A test whose ROM prints a measurement instead of a verdict pins Mesen2's capture of the same frame (0-px diff), named in the test comment, never NESER's own output. The read_joy3 tests now do. Other `setup_rom_console_test!` lines pinning a number are worth checking the same way.
**Seen before.** nr-nuf ("A test pass count tuned to NESER's own output read as a regression against the reference").

## The recorded nr-046 NMI difference made checkpoint comparisons look like regressions

**What happened.** After the DMC fixes, Ninja Gaiden (frame 900) and Joe & Mac (2700) newly differed from Mesen2 at checkpoints, and 8 ROMs from the bead's list still differed. Their NMI clock logs differed from Mesen2's from NMI 7 on, by a few cycles. The cause was the deliberate nr-046 difference: NESER delays an NMI inside a taken branch, Mesen2 does not. With that one line changed in a throwaway build, 20 of 25 ROMs matched at every checkpoint and none was worse than main.
**Why.** Games that wait for NMI in a branch loop drift by a few cycles every frame in spec mode, so any timing change moves their game state somewhere new.
**Cost.** About an hour of spec-mode checkpoint runs that could not tell a real regression from nr-046 noise.
**Prevent by.** scripts/reference_capture/README.md, "Tracing against Mesen2": add that comparisons against Mesen2 are judged with an NMI-the-Mesen2-way build (latch NMI in `after_cpu_cycle` even when `skip_interrupt_latch_this_cycle` is set) before and after the change, and the spec-mode result is reported separately.
**Seen before.** nr-046, nr-3jh.
