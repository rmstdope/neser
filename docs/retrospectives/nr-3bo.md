# nr-3bo retrospective

## The CI-wait loop from `produce-bead` returned at once

**What happened.** `produce-bead` *Waiting, without ending your run* prescribes
`until <cond>; do bd heartbeat <id>; sleep 30; done` inside one Bash call. In this Claude Code
session, foreground `sleep` is blocked by the Bash tool, so every `sleep 30` failed at once.
Loops meant to wait for up to eight minutes ran for about a second. Each printed a
still-pending CI table that looked like a completed wait. The gate-log waits looked slow only
because `bd heartbeat` took time. The fault showed only when `date -u` came back almost
unchanged between two supposedly long loops.

**Why.** The Bash tool in this harness refuses foreground `sleep`, and its own description
says to use Monitor with an until-loop instead. The skill's recipe predates that restriction.

**Cost.** About ten wasted poll rounds while CI was queued behind other fleet runs, and a
false reading that CI had been stuck for half an hour.

**Prevent by.** `.cerebro/cerebro/skills/produce-bead/SKILL.md`, *Waiting, without ending your
run*: for a Claude Code session, wait on CI with the Monitor tool (a poll loop that emits one
line per finished check and exits when none is pending; `sleep` works inside Monitor), or with
a Bash `run_in_background` until-loop. Keep the plain `sleep` loop only for CLIs that allow it.

**Seen before.** Nothing like it in `docs/retrospectives/`.
