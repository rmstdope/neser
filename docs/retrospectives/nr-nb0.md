# nr-nb0 — retrospective

- **Implementer:** Gambit
- **Date:** 2026-10-08
- **PR:** #3349

## A fifth bead rediscovered the nr-046 NMI rule by tracing

**What happened.** Five of the bead's six ROMs fall a frame behind Mesen2 only because NESER delays an NMI that lands in the last cycle of a taken branch, which nr-046 kept on purpose. I found it by building `timing_trace`, diffing NMI logs and exec traces, and then running a throwaway build with Mesen2's rule. The bead's own note said to rerun after nr-3jh landed, which suggested nr-3jh had changed that rule. It had not: nr-3jh attributed the same signature (handler at clk 176307 vs 176304) to nr-046 and left it.
**Why.** The bead was filed from a checkpoint sweep that does not separate decided differences from new ones. No switch exists to rerun a ROM under Mesen2's NMI rule, so every investigator rebuilds the same one-line patch by hand.
**Cost.** About three hours of tracing and two six-ROM Mesen2 sweeps, plus a navigator question whose answer was already on record.
**Prevent by.** A way to rerun `compare_mesen2` with NESER under Mesen2's NMI rule (for example a debug-only CLI flag or environment variable read where `skip_interrupt_latch_this_cycle` is handled, passed through with `--neser-arg`). A sweep bead would then list only ROMs that still differ with the rule switched, and the "Tracing against Mesen2" section of `scripts/reference_capture/README.md` would name that rerun as the first step. The patch itself is in nr-nb0's notes.
**Seen before.** nr-sjt, nr-3wg, nr-8px and nr-3jh: the same difference, each found again by tracing.

## The fix that shipped was not the bug the bead describes

**What happened.** The first divergence in Metal Mech's trace was a real DMC bug: the power-on sample was 0 bytes at $0000 instead of 1 byte at $C000. Fixing it made the boot cycle-exact but changed no checkpoint. Only the experiment showed that the NMI rule alone explains the pictures.
**Why.** The first divergence in a trace is not necessarily the one that reaches the screen. The NMI clock log aligned again a few frames after the DMC stall.
**Cost.** One six-ROM sweep spent before the experiment that answered the question.
**Prevent by.** The same rerun under Mesen2's rule as above, done before reading any trace.
**Seen before.** Not established.
