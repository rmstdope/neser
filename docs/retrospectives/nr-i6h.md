# nr-i6h retrospective

## The pre-commit hook skips ruff even when `.venv` has it

**What happened.** The first commit of this bead printed `WARNING: ruff not found on PATH; skipping Python formatting.` and went in unformatted, although `./scripts/setup-venv.sh` had just built `.venv` with ruff 0.16.1. The formatting was only caught because `ruff format --check` ran by hand before the commit. The second commit was formatted only because `.venv/bin` was put first on `PATH` for that one command.
**Why.** `.githooks/pre-commit` looks for `ruff` with `command -v ruff`. A producer never activates the venv, and the gate deliberately calls `.venv/bin/python` directly. So the venv this bead now builds in every worktree is invisible to the hook.
**Cost.** A few minutes. Left alone, it lets unformatted Python reach the gate's `ruff format --check` leg, or CI.
**Prevent by.** `.githooks/pre-commit` prefers `.venv/bin/ruff` (from `git rev-parse --show-toplevel`) and falls back to `ruff` on `PATH`, the same way `scripts/gate-full.sh` uses `.venv/bin/python`.
**Seen before.** None found.
