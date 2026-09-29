# nr-m6o — retrospective

- **Implementer:** Nightcrawler
- **Date:** 2026-09-29
- **PR:** #3325

## A release build's `--trace-cpu` printed interrupts but no instructions

**What happened.** To find where Super Aladdin's CPU phase drifts from Mesen2's, I ran
`target/release/neser --headless --frames 6 --trace-cpu <rom>`. It printed two `[CPU] NMI` lines
and nothing else, so it looked as if tracing worked and the window was simply empty. The
instruction lines come only from `target/debug/neser`: the `exec` trace in
`src/nes/cpu/cpu/execute.rs` sits under `#[cfg(debug_assertions)]`, while the NMI and IRQ lines
are compiled in every build.

**Why.** Instruction tracing is compiled only into debug builds, and `neser --help` still
describes `--trace-cpu` as "Enable CPU trace output" without saying so. In a release build the
interrupt lines make the output look complete rather than empty.

**Cost.** About ten minutes and one release-plus-debug rebuild cycle, before the traces showed
that the instruction streams were identical and the NMI was taken one instruction late (nr-3jh).

**Prevent by.** Put "(instructions in debug builds only)" in the `--trace-cpu` help text in
`src/nes/console/config/cli.rs`, or print one stderr line when `--trace-cpu` runs on a release
build. nr-kyu proposed the same and it has not been done yet; this is the second time.

**Seen before.** nr-kyu.
