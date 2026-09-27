# nr-a0g — retrospective

- **Implementer:** Cyclops
- **Date:** 2026-09-27
- **PR:** #3234

## With the hook active, a fresh worktree still commits unformatted Python

**What happened.** `core.hooksPath` was `.githooks` and the pre-commit hook ran, but it printed `WARNING: ruff not found on PATH; skipping Python formatting.` and let the commit through. The first `ruff format --check scripts` in the gate then failed on the new test file.
**Why.** The hook looks for `ruff` on PATH only. This tree had no `.venv` yet (the gap nr-qoi, nr-6e9 and nr-hab.1 record, which nr-i6h closed on main while this PR was open), and even with one, `.venv/bin` is on PATH only in a shell that activated it. This bead makes the hook run; it does not give the hook a Python formatter.
**Cost.** One extra formatting commit and a rerun of the Python legs, a few minutes.
**Prevent by.** Have `.githooks/pre-commit` use `.venv/bin/ruff` when it exists (every worktree now builds `.venv`), and fail rather than warn when Python is staged and no ruff is found.
**Seen before.** nr-qoi, nr-6e9, nr-hab.1 (the missing `.venv`, seen from the gate's side rather than the hook's).

## `build-workload --classify` refused to classify the diff

**What happened.** `build-workload: no rust_paths in .cerebro/project.conf - cannot classify safely.`
**Why.** `.cerebro/project.conf` still declares no `rust_paths`.
**Cost.** A minute: I ran `disk-preflight --workload rust` myself.
**Prevent by.** The prevention nr-273 names: declare `rust_paths` in `.cerebro/project.conf`.
**Seen before.** nr-273.
