# nr-y7yu — retrospective

- **Implementer:** Wolverine
- **Date:** 2026-10-24
- **PR:** #3371

## A parked tuning bead resumed with measurements taken under conditions main had since changed

**What happened.** The bead rebalances the two web-integration shards. Its weights (29:30) came
from CI logs taken with two Playwright workers per shard. It was parked for a scope decision
because flaky 45 s timeouts decided the walls. While it was parked, nr-oi18 (#3374) merged one
worker per shard on CI, which removed those flakes and changed every duration. On resuming, I
rebased onto main as it stood at the start of the pass and reran the gate. While the review ran,
nr-oi18 merged, and I did not look at what had changed in the files that set the measured
conditions. The cold review caught it: the branch conflicted with main, and every figure in the
comments, the docstring and the plan was two-worker data. On the one-worker run (40049199903) the
cut still held: test steps of 406 s and 377 s, no retries.

**Why.** Resuming a returned plan (`produce-bead` step 2) means reading what shipped *for this
bead* and amending to the newest note. Nothing asks whether main has changed the *conditions*
the plan's numbers were measured under. For a bead whose acceptance is a timing, that is the
whole input.

**Cost.** One review round, a second rebase, one extra CI run, and a comment-only commit to
replace stale figures.

**Prevent by.** `produce-bead` step 2, on a returned plan whose *Validation* is a measurement:
run `git log <old base>..origin/<default> -- <the files that set the measured conditions>` (here
`playwright.config.ts` and the job in `.github/workflows/ci.yml`). If anything merged there,
re-measure on the rebased head before reusing any number from the notes.

**Seen before.** nr-oi18 (the change that moved the conditions; its own retrospective covers the
flake's cause, not this). None found for a stale measurement.
