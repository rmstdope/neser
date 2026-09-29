# nr-6gs — retrospective

- **Implementer:** Bishop
- **Date:** 2026-09-29
- **PR:** #3322

## A timing change that fixed one game exactly was wrong across the library

**What happened.** Snake's Revenge's DMC fetches landed 8 CPU cycles later than Mesen2's. Starting the DMC timer 8 cycles into its period at the first opcode, as Mesen2 does, gave an identical instruction trace through frame 268. All 12 checkpoints matched, and the hardware test ROMs still passed. A 701-ROM sweep (main against the branch, then Mesen2 for every ROM whose picture changed) showed it fixed 3 ROMs and made 9 worse. 4 of those 9 had matched Mesen2 at every checkpoint before. Top Gun 2 then showed the real difference: *when* a DMC request becomes visible to the CPU, which one constant phase cannot fix for both games. The change was dropped and filed as nr-xgb.

**Why.** Many DMC fetches that differ by one cycle still land on the same get cycle, because the fetch waits for a get cycle anyway. So one game's trace can only pin a timing difference up to that alignment. The test ROMs do not cover the phase.

**Cost.** About 1.5 hours: the fix, its test and the re-pinned latency count were written, gated and then reverted.

**Prevent by.** For any change to CPU/APU/PPU timing, run the whole-library old-against-new sweep *before* writing the test and the commit, not after. It only needs Mesen2 captures for the ROMs whose picture changed, so it runs in about 15 minutes on 14 cores. The place to say so is `scripts/reference_capture/README.md`, *Mesen2*, next to nr-f6o's note about recording the region.

**Seen before.** nr-f6o (its review asked for the same library rerun after the fact).

## Mesen2's `emu.getState()` APU fields are stale mid-frame

**What happened.** To compare DMC timer phases I read `apu.dmc.timer.timer` and `apu.dmc.bitsRemaining` from `emu.getState()` in an NMI callback. The values did not fit the cycle count taken from the same call.

**Why.** Mesen2 runs its APU lazily (`NesApu::Run` catches up only at register accesses, at the frame end or when an IRQ is due). A state snapshot shows the APU as of its last catch-up, not as of `cpu.cycleCount`.

**Cost.** About 10 minutes, and a wrong lead that was nearly acted on.

**Prevent by.** To time APU events in Mesen2, use a memory callback on the event itself (a read callback on the DMC sample range gives each fetch's `cpu.cycleCount`), not a state snapshot. Worth a line in `scripts/reference_capture/README.md`, *Mesen2*.

**Seen before.** nr-4lq and nr-dh7 note that `emu.getState()` is slow, not that it is stale.
