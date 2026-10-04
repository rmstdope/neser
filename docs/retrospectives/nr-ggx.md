# nr-ggx — retrospective

- **Implementer:** Cyclops
- **Date:** 2026-10-04
- **PR:** #3333

## A Mesen2 script wrote past its last line, and only a stdout shim showed it

**What happened.** `mesen2_exec_trace.lua` closes its file and calls `emu.stop(0)` when the
window ends. Run by hand, every trace had the right length. In `scripts.test_mesen2_traces`,
which swaps `io.open` for a shim that prints to stdout, all four Mesen2 exec traces ran past
NESER's by hundreds of lines.
**Why.** `emu.stop()` is not immediate: callbacks keep firing after it. In the real run they
wrote to a closed file and Lua dropped the error, so the extra lines were lost silently rather
than not produced.
**Cost.** One end-to-end round, about 5 minutes. It would have been a wrong trace length on
any run where the file stayed open.
**Prevent by.** Every Mesen2 script that stops early sets a `done` flag that its callbacks check
first (both trace scripts now do). This is listed under "Mesen2 Lua behaviours" in
`scripts/reference_capture/README.md`.
**Seen before.** None found. nr-hg7 met a different silent-script failure (file access off).

## The plan assumed Mesen2 needed PC collapsing; it needed a per-instruction rule on NESER's side

**What happened.** The plan, following nr-dh7's recipe, collapsed consecutive equal PCs.
Measured, a Mesen2 `exec` callback fires exactly once per instruction. NESER's samples before
an SNES interrupt-dispatch step, a WAI wait or an NES OAM-DMA tick sit on a PC that has not
executed yet. The fix was a second CPU counter (`instructions_executed`), so that only a sample
followed by an executed instruction is written. Even then, a stall at an instruction boundary
(OAM DMA, SNES DRAM refresh) is charged to opposite sides of it by the two emulators, so the
diff had to allow one-line excursions.
**Why.** nr-dh7's collapse covered the same effect from the other side, and nobody had written
down which side the duplicates came from.
**Cost.** About 20 minutes of re-planning and one extra counter in both cores.
**Prevent by.** The "Reading the diff" paragraph in `scripts/reference_capture/README.md` now
lists the three known stamp differences. Probe Mesen2's callback granularity on a small ROM
(a 25-line `exec` print) before designing against it.
**Seen before.** nr-dh7 and nr-4cl (where an exec callback's clock sits).

## A constant clock offset was read as a clock origin, but it was a real NMI-entry difference

**What happened.** On `window-precalculated-single.sfc` the NMI logs differed by a constant 6
master clocks. I wrote it down as each emulator's clock origin, although my own exec trace from
power-on matched at offset 0. The cold review caught the contradiction. The 6 clocks are NESER
entering each NMI one fast cycle late or early against Mesen2, filed as nr-7pk.
**Why.** The diff took the first line's offset as its baseline, so the number looked like
calibration rather than a measurement.
**Cost.** One review finding, and a `--baseline` option added afterwards.
**Prevent by.** `diff_timing_traces --baseline` with the README's expected baselines (0 on the
NES; the `exec --from-nmi 0` offset on the SNES), and the end-to-end test pins them.
**Seen before.** nr-4lq (a false one-frame lag from comparing at each emulator's own event).
