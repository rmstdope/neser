# nr-e08 — retrospective

- **Implementer:** the navigator's hand-driven session
- **Date:** 2026-09-27
- **PR:** #3260

## A "wait for CI" loop that returned before CI finished, twice

**What happened.** The release skill's wait for the release PR's checks was first written as
`until gh pr checks <n> | grep -qv pending`, which returns as soon as any one line is not
pending; the review caught it. The replacement,
`gh pr checks <n> --json state -q 'all(.[]; .state != "PENDING")'`, returned true while two
checks were still running, because `state` reads `IN_PROGRESS` or `QUEUED` for a running check
and `PENDING` only before it starts. The monitor watching this very PR fired early on it.
**Why.** `gh pr checks --json` has two fields: `state` is GitHub's raw check state, `bucket`
folds every unfinished state into `pending` and every finished one into `pass`, `fail`,
`skipping` or `cancel`. Only `bucket` is a completion test.
**Cost.** One review round and one CI cycle; had it shipped, a release could have merged and
tagged with CI still running, since branch protection requires only two of the checks.
**Prevent by.** `.claude/skills/release/SKILL.md`, step 4, now reads `bucket` and says why. Any
new wait on `gh pr checks` should copy that line rather than the human-readable output.
**Seen before.** none found.

## The branch was rebased onto a newer main by something outside the session

**What happened.** Between pushing `a40e3800` and opening the PR, the worktree's branch was two
commits ahead of and two behind its own upstream: the same two commits, rebased onto a main that
had moved twice. Nothing in this session ran a rebase.
**Why.** Not established. The rebase was content-identical, so it was pushed with
`--force-with-lease` and the PR built on current main.
**Cost.** Ten minutes of checking that the diff was unchanged; no CI cycle.
**Prevent by.** Before `gh pr create`, run `git status -sb` and compare `HEAD` with the pushed
branch; a divergence that is only a rebase is pushed with `--force-with-lease=<branch>:<old sha>`,
anything else is investigated. Worth a line in `produce-bead` once whoever rebases is known.
**Seen before.** none found.
