# nr-oi18 — retrospective

- **Implementer:** Bishop
- **Date:** 2026-10-24
- **PR:** #3374

## The first fix named a real app bug as the cause of a CI flake it did not cause

**What happened.** The first pass found a genuine bug in `web/src/app.ts`: pausing and resuming
within one animation frame started a second frame loop. Its reproduction test failed before the
fix and passed after it. The pass logged it on the bead as the cause of "Failed to find browser
context", reasoning from the low fps in the CI traces. The cold review flagged that this chain was
inferred, not proven. CI runs at that head were still flaky: 10–13 flaky tests per run on two
workers (runs 40033049230, 40035552969). A diagnostic run with one Playwright worker per shard
(40037895653) had nothing flaky, and its larger shard took 455 s against 991 s.

**Why.** Each Chromium on CI renders WebGL through a SwiftShader GPU process that takes about 1.6
cores. Playwright's default of two workers on the four-vCPU runner starved Chrome's browser process.
Closed renderers lingered as zombies for up to 40 s, and new contexts hung until the test budget
ran out. The frame-loop bug only added load on top. The repo's tests could not tell an amplifier
from the cause: a test proves that a bug exists, not that it is the one behind a symptom.

**Cost.** Two extra review rounds, five CI runs on the branch, a superseded bug log on the bead,
and a rebase across nr-fli1.1's pnpm migration before the merge.

**Prevent by.** In `fix-bug`'s contract, step 3 ("fail for the right reason"): for a CI-only
symptom such as a flake or a timeout, the bug log names a CI run that shows the symptom gone,
compared against runs that show it, before it calls anything the cause. A local reproduction of a
neighbouring bug is not enough.

**Seen before.** nr-1gg: Playwright tests failed locally with the default workers and passed with
`--workers=1`, which was the same oversubscription seen off CI.
