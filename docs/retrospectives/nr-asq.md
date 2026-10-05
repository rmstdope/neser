# nr-asq — retrospective

- **Implementer:** Gambit
- **Date:** 2026-10-05
- **PR:** #3344

## A 300-frame checkpoint sweep cannot see a one-line jitter

**What happened.** The bead said Crash Dummies differed from Mesen2 at the 1200 and 1800 checkpoints. On main at 030a0959 all 12 checkpoints (300..3600) matched with 0 px, both before and after the fix. The bug showed only when every frame was compared. Then 8 of frames 860-1000 and 131 of frames 1001-2400 differed: on scattered frames the text under the split sat one line higher.

**Why.** The divergence depends on IRQ latency, so it appears on some frames and not others. Any fixed set of checkpoints can land on frames that agree.

**Cost.** One round of doubt about whether the bug still existed, and about 15 minutes of dense captures.

**Prevent by.** `scripts/reference_capture/README.md`, *Comparing a ROM*: say that a jitter or flicker report needs every frame across the reported scene, not the 300-frame checkpoints. Say which tool does that: NESER's `--capture-every 1`, plus one Mesen2 run that saves a range of frames (below).

**Seen before.** none found.

## `compare_mesen2` starts Mesen2 again for every frame and times out late frames

**What happened.** A dense range (`--frames $(seq 1000 1060)`) took over 10 minutes for 23 frames: every frame is a fresh Mesen2 run from power-on. In the 9-game MC-ACC sweep, several games printed `capture failed: Mesen2 saved no screenshot` at frames from 600 to 2100. A frame that late does not finish inside the pinned `--timeout=30` when other fleet sessions load the machine. I worked around both with a scratch copy of `mesen2_capture.lua`. It saves every frame (or every 300th) from CAPTURE_FIRST to CAPTURE_LAST in one run with `--timeout=900`: 1400 frames in a few minutes, with no failures.

**Why.** `mesen2_capture.lua` captures one frame per run, and `compare_mesen2` pins a 30-second timeout whatever the frame number.

**Cost.** About 20 minutes of failed or slow captures, and a sweep that had to be redone.

**Prevent by.** In `scripts/reference_capture/`, let `mesen2_capture.lua` take a list or range of frames, and have `compare_mesen2` capture all of `--frames` in one Mesen2 run, with a timeout that scales with the last frame.

**Seen before.** none found.
