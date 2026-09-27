# nr-cxx — retrospective

- **Implementer:** Rogue
- **Date:** 2026-09-27
- **PR:** #3269

## The planned RED test passed without the fix, and so did the test it copied

**What happened.** The plan's RED test (a byte write to TM0CNT_L mid-instruction while TM0 is
FFFF must not tick TM0) passed against the unfixed byte path. It copied the setup of the existing
`active_timer_reload_from_ffff_defers_current_instruction_tick` in `src/gba/bus/gba_bus.rs`.
Commenting out `defer_active_timer_reload_write_cycle` in the halfword path showed that the
existing test also passed without the hook it is named after.

**Why.** Both tests enable the timer with `write16(TMxCNT_H, …)` and go straight into
`begin_cpu_instruction`. The enable sets `timer_start_delay_pending`, and the first
`step_after_cpu_instruction` swallows those cycles. So the tick the defer is meant to suppress
never happens, with or without the defer. Running the timer past the start delay first
(`bus.step(4)`, counter still FFFF with reload FFFF) makes the defer the only thing holding the
tick back, and the byte case then fails as it should.

**Cost.** About ten minutes. The bigger cost was already paid: the defer hook had no test that
could fail since it landed (93407d1d).

**Prevent by.** A GBA timer-timing test that starts from a freshly enabled timer steps past the
start delay (`bus.step(n)`) before `begin_cpu_instruction`. The every-width tests in
`src/gba/bus/io_write.rs` do this, and this PR deletes the blind test. For any timing-hook test,
the RED step removes the hook and watches the test fail, rather than trusting a copied setup.

**Seen before.** none found
