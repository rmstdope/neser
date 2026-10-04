# nr-3jh — retrospective

- **Implementer:** Nightcrawler
- **Date:** 2026-10-04
- **PR:** #3332

## A Mesen2 Lua trace counted by `startFrame` traces the frame before the capture of the same number

**What happened.** To explain Rad Racer's difference at checkpoint 2400, I traced PPU writes
and PPU fetches with a Mesen2 Lua script that counted `startFrame` events and logged while the
counter read 2400. Those writes hit dots where nothing differed. The picture the capture script
saves at `startFrame` N is frame N, but it was rendered while the counter still read N-1. The
lines that mattered were in the counter-2399 trace.
**Why.** `mesen2_capture.lua` takes frame N at the N-th `startFrame`, after rendering has
finished. A trace keyed on the same counter is one frame ahead of the picture. NESER's
`frame_count()` behaved the same way in my env-gated log, so the two traces agreed with each
other and only disagreed with the screenshots.
**Cost.** About half an hour of dot-level analysis on the wrong frame.
**Prevent by.** The "Finding the first divergent frame" section that nr-046 asked for in
`scripts/reference_capture/README.md` should say this: a trace for checkpoint N logs while a
`startFrame` counter reads N-1.
**Seen before.** nr-mxn (the capture-side frame numbering); nr-046 (the RAM-dump method).

## A test pass count tuned to NESER's own output read as a regression against the reference

**What happened.** `test_scanline` allowed "at most 20" bad pixels in scanline.nes. The fix
that made NESER match Mesen2 at 0 px on every checked frame failed it with 84. The navigator
chose to re-pin it to Mesen2-approved CRCs.
**Why.** The limit had been set from NESER's output, not from a reference capture, so it
encoded NESER's earlier timing.
**Cost.** One gate run and one navigator question.
**Prevent by.** Visual test ROMs pin a reference-approved golden (the
`nes-hardware-research` skill, Authorities tier 3), never a tolerance count.
**Seen before.** Nothing like it in `docs/retrospectives/`.

## A latent debug-build overflow surfaced as a flaky gate test

**What happened.** The Vs. coin wasm test failed once in the full gate and passed when rerun
alone. The cause was `addr - 0x100` in the ABSYW dummy read, which overflows when base+Y
wraps past $FFFF. The test's zero-filled ROM runs from random power-on RAM and reaches that
case only sometimes.
**Why.** Unit tests run with overflow checks, and that one test executes arbitrary code from
random RAM.
**Cost.** One extra gate run; fixed in this PR.
**Prevent by.** When a gate test fails once and passes on a rerun, read its panic line before
calling it a flake.
**Seen before.** Not established.
