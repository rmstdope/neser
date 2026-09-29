# nr-4cl — retrospective

- **Implementer:** Gambit
- **Date:** 2026-09-29
- **PR:** #3319

## Mesen2 Lua read callbacks never fired on PPU registers, and the exec-callback clock was misread

**What happened.** To see which H value Mesen2 latches through `$2137`/`$213C`, I tried
`emu.addMemoryCallback(cb, emu.callbackType.read, …)` on `$2137-$213F` and on `$802137-$80213F`,
with and without `cpuType`/`memType`. It never fired, not even once in 300 frames, while `exec`
callbacks in the same script did. Working around it by sampling `cpu.a` in an `exec` callback on
the instruction after the load worked. Separately, I first read Mesen2's
`SnesMemoryManager::Read` (which calls `ProcessMemoryRead` after the cycle) as the point where the
`exec` callback fires, and concluded that NESER and Mesen2 were in CPU lockstep. They were not.
The `masterClock` an `exec` callback reports is the clock *before* the opcode fetch, the same
point NESER's loop prints before `run_tick()`. That is how the traces at boot line up exactly.

**Why.** Not established for the read callbacks. The misreading was mine: the trace data (boot
traces identical in both) already said which convention holds.

**Cost.** About 30 minutes, including one wrong theory about the H-latch formula.

**Prevent by.** The "Tracing against Mesen2" section that nr-dh7 asked for in
`scripts/reference_capture/README.md` should also say that read callbacks do not fire on PPU
registers (sample the register in an `exec` callback on the next instruction instead), and that
an `exec` callback's `masterClock` is the clock before the opcode fetch.

**Seen before.** nr-dh7 (write callbacks with `cpuType` never fired).

## Two hand-backs said the capture comparison could not run; here it ran as written

**What happened.** The bead's notes carried two hand-backs, both saying the fresh NESER/Mesen2
rerun was blocked before execution because its PNG outputs matched an ignored file type. In this
session the exact command wrote and read every PNG without any block, and the rerun found the
remaining divergence at once.

**Why.** Not established. The block came from the other session's environment (a pre-tool hook),
not from the repository.

**Cost.** The bead sat handed back for about a day.

**Prevent by.** In `skills/fix-bug` *When to hand back*: a tool or sandbox block in the
producer's own environment is an environment fault to report to the navigator. It is not a
property of the bead, and the bead should not be parked as unreproducible because of it.

**Seen before.** None found.
