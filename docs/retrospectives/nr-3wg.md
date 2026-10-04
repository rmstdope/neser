# nr-3wg — retrospective

- **Implementer:** Shadowcat
- **Date:** 2026-10-04
- **PR:** #3339

## I re-derived a Mesen2/NESdev difference that nr-046 had already recorded and put to the navigator

**What happened.** Tekken 2's attract-mode fight drifts from Mesen2 from frame 1307. I traced it
with per-frame RAM dumps and exec traces to an NMI arriving in a taken same-page branch's second
cycle. NESdev delays that NMI by one instruction; Mesen2's `BranchRelative` delays only IRQ. I
confirmed it by experiment (Mesen2's rule gives 0 px at all 12 checkpoints) and asked the
navigator, who chose NESdev. Only when writing this file did I find the same cause, the same
experiment and the same decision in `docs/retrospectives/nr-046.md`, plus a doc comment on
`skip_interrupt_latch_this_cycle` in `src/nes/cpu/cpu/mod.rs`.
**Why.** The fact lived in a retrospective and on a private field, and neither is where you look
when a bead says "diverges from Mesen2". `.cerebro/traps.md`, which planners and implementers read
first, is still empty. nr-3wg's own description (filed from nr-55b) guessed "CPU-cycle or IRQ
timing", without naming this known difference.
**Cost.** About two hours of tracing and a second navigator question for a decision already taken.
**Prevent by.** A `.cerebro/traps.md` entry: "Mesen2 takes an NMI that a taken same-page branch
should delay (NESdev CPU interrupts); NESER keeps NESdev (nr-046, nr-3wg). A Mesen2-parity drift in
a game that waits for vblank in a `BEQ`/`BNE` loop is this until shown otherwise: try Mesen2's rule
in `timing.rs` as an experiment before tracing." The new test,
`test_taken_branch_without_page_cross_delays_nmi_like_a_two_cycle_instruction`, now pins the
behaviour directly, so an accidental "fix" toward Mesen2 fails by name and not just through Ruder.
**Seen before.** nr-046 (the same difference, found in Vs. Duck Hunt).
