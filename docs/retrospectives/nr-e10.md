# nr-e10 — retrospective

- **Implementer:** Shadowcat
- **Date:** 2026-10-05
- **PR:** #3345

## Another bead shipped the same MC-ACC mapper change while this one was being built

**What happened.** nr-e10 (Acclaim MC-ACC games split on a different line than Mesen2) and
nr-asq (Incredible Crash Dummies' intro, filed against a different game) were in progress at the
same time. Both pinned the cause to mapper 4 submapper 3 counting falling A12 edges through a ÷8
prescaler, and both added that mode to `MMC3Mapper`. nr-asq merged as #3344 while nr-e10 was
validating against Mesen2. The PR conflicted in `src/nes/cartridge/nintendo/mmc3.rs`, and only the
cold review spotted that main already had the feature under different names. nr-e10 was rebased
down to its PPU bus fixes, which nr-asq did not have.

**Why.** nr-asq's title names its game, not the board, and the MC-ACC cause showed up only during
its investigation, so nothing on the board linked the two beads. The bugfixer's start-up reads its
own bead and `git log` for its own id, not open or recently merged work on the same files or the
same mapper.

**Cost.** A duplicate implementation and four duplicate tests, written and validated (12-checkpoint
Mesen2 comparisons for three ROMs), then thrown away. After that a rebase, a second full gate and
a second validation run, about an hour of wall-clock.

**Prevent by.** In `fix-bug`, once the cause is pinned to a component and before writing the fix,
run `git log origin/main --since=<bead start> -- <file>` on the files the fix will touch, and
`bd list --status in_progress -l <area>` (e.g. `mapper`). When a sibling bead touches the same
component, read its diff first. nr-asq could also have noted its MC-ACC cause on nr-e10, which
lists the MC-ACC boards by name.

**Seen before.** nr-dh7 (a concurrent bead fixed the same code), nr-zdy.2 (parallel beads editing
the same places).
