# nr-4lq: Batman Returns' RNG drift was found by a slow bisection

## The CPU-timing bisection against Mesen2 spent an hour where seconds would do

**What happened.** I bisected when NESER's CPU first drifted off Mesen2 by sampling the master
clock through a Mesen2 Lua `exec` callback on the whole address range, calling
`emu.getState()` on every instruction. It ran at about 250k master clocks per second. Reaching
frame ~1030 took over an hour, spread across several runs. Logging only the clock at each
entry to the NMI handler (an `exec` callback on the single handler address) produced the same
first-divergence answer through frame 3600 in 8 seconds. Every drift in this bead (from JSL,
RTL, JSR and RTI) moved the next NMI entry, so that one clock per frame was enough to locate
the frame to fine-trace.

A second trap cost less. The first attempt compared WRAM dumps taken at each emulator's own
"frame" event. NESER's `is_ready_to_render` and Mesen2's `startFrame` fire at different points
in the frame, so the dumps showed a false one-frame lag even though the CPUs were
cycle-identical at that point.

**Why.** `emu.getState()` builds the entire console state table on every call. Calling it per
instruction dominates the run time.

**Cost.** About 75 minutes of wall-clock tracing, plus one wrong lead (the WRAM "lag").

**Prevent by.** In `snes-hardware-research`, next to the Mesen2 capture recipe: bisect a
timing divergence by comparing the master clock at each NMI-vector entry in both emulators
(a Lua `exec` callback on the handler address; in NESER, `master_clock_for_tests()` whenever
the PC is at the handler). Then fine-trace only the frame before the first mismatch, arming the
per-instruction hook from a `startFrame` callback near that clock. Compare state by master
clock, never by each emulator's own frame event.

**Seen before.** Nothing like it yet.
