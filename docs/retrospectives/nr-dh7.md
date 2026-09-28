# nr-dh7: Tracing SNES timing against Mesen2 through its Lua API

## What happened
Finding a 2-master-clock divergence meant comparing NESER and Mesen2 per frame, then per
instruction. The Mesen2 side went through `--testRunner` Lua scripts, and three of its behaviours
cost several rounds before any data came back:

1. `emu.addMemoryCallback(cb, emu.callbackType.write, lo, hi, emu.cpuType.snes)` never fired.
   The same call without the `cpuType` argument did fire.
2. A callback on `0x2100..0x21FF` sees only bank `$00`. Writes this game makes through bank `$80`
   (FastROM) or with DBR=`$81` were missing, which at first looked like NESER writes that Mesen2
   did not make.
3. `emu.eventType.endFrame` and NESER's `is_ready_to_render()` fall on opposite sides of the CPU
   writes at the very start of scanline 225. Per-frame diffs showed spurious differences until
   both sides skipped scanline 225 when choosing "the first write of the frame".

What did work: `memoryManager.masterClock` and `memoryManager.hClock` from `emu.getState()` agree
exactly with NESER's `total_master_clocks` and intra-line clock. An `exec` callback over
`0x000000..0xFFFFFF`, collapsed to PC changes, diffs cleanly against a NESER loop that prints
`cpu_pc_for_tests()` and `master_clock_for_tests()` before each `run_tick()`. Once RAM was
sampled at the same point (`endFrame` rather than `startFrame`), it isolated the divergent
instruction in two runs.

## Why
Not established for (1). (2) and (3) follow from how Mesen2 keys callbacks (by bus address) and
where it raises EndFrame. Neither is documented next to our capture script.

## Cost
About an hour of instrumenting before the first usable trace.

## Prevent by
`scripts/reference_capture/README.md`, a "Tracing against Mesen2" section that records the three
points above and the per-instruction clock-diff recipe (Lua `exec` callback plus the
`cpu_pc_for_tests`/`master_clock_for_tests` loop).

## Seen before
Nothing like it in `docs/retrospectives/`.

# nr-dh7: A concurrent bead fixed the same code, and the rebase silently doubled a cycle

## What happened
While #3298 was in review, nr-4lq (#3296) merged the same reordering of RTS/RTL/RTI/JSR/JSL,
reached from a different game (Batman Returns). The rebase conflicted in five hunks. Git
auto-merged a sixth, JSR abs, without flagging it: main put the internal cycle before
`let ret`, this branch put it after, and the merge kept both. JSR abs became 7 cycles. Taking
main's side of every flagged hunk looked complete. Only the per-cycle speed test caught it
(`[8,8,8,6,6,8,8]`), and the game was back to 1% different at frames 1200 and 1500.

## Why
Two independent fixes inserted the same one-line call at adjacent but different lines. That is
not a textual conflict, so git merged both.

## Cost
One extra build-and-compare round; it would have shipped a JSR regression without the test.

## Prevent by
`produce-bead` *Merging*: after resolving a rebase, run the bead's reproduction test and the
changed module's tests before pushing. A clean `git rebase --continue` proves only that no
textual conflicts remain.

## Seen before
Nothing like it in `docs/retrospectives/`.
