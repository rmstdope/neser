# nr-nts2 — retrospective

- **Implementer:** Rogue
- **Date:** 2026-10-24
- **PR:** #3372

## A test that reads the CI workflows passed locally and failed in CI

**What happened.** `scripts/test_ci_job_timeouts.py` reads `.github/workflows/ci.yml` and was
green under `./scripts/gate-full.sh` on the branch. In CI, python-tests failed at
`test_the_job_parser_finds_the_known_jobs`. CI tests the pull request's merge ref, and #3368 had
landed on main during the gate. That PR moved web-integration's apt install into
`scripts/ensure_playwright_deps.sh`, so the merged ci.yml no longer had the spelling the guard
looked for. `git fetch origin pull/3372/merge` and a diff of its ci.yml against the branch showed
the change.
**Why.** The local gate tests the branch, not the branch merged with main. A test whose subject
is a file other PRs edit often (the workflows) is the most exposed to that gap.
**Cost.** One CI cycle (about 15 minutes), a rebase and one of the three fix attempts.
**Prevent by.** For a bead whose tests read `.github/workflows/*.yml`, run
`git fetch origin main && git log HEAD..origin/main -- .github/workflows` after the gate and
before opening the PR, and rebase if it lists anything. This could be one line under *Building*
in `produce-bead`.
**Seen before.** None found.
