# nr-6e9 — retrospective

- **Implementer:** Bishop
- **Date:** 2026-09-25
- **PR:** #3191

## The first full gate failed on `cargo fmt --check` although the commit hook should have formatted

**What happened.** The first `./scripts/gate-full.sh` run stopped at its first leg, `cargo fmt --all -- --check`, on a test file committed a moment earlier.
**Why.** `git config core.hooksPath` printed nothing in the prepared worktree, so `.githooks/pre-commit`, which auto-formats staged Rust, never ran. CLAUDE.md says `core.hooksPath` points at `.githooks`. The prepared worktree does not set it.
**Cost.** One full gate run lost (about ten minutes before it reached the failure), plus a `cargo fmt` commit.
**Prevent by.** Have `scripts/prepare-worktree` (or `install_shell` in `.cerebro/project.conf`) run `git config core.hooksPath .githooks` in each new worktree.
**Seen before.** None found.

## wasm-pack test failed locally with "http status: 404" from ChromeDriver, a third time

**What happened.** The `wasm-pack test --headless --chrome` leg failed before any test ran, with `driver status: signal: 9 (SIGKILL)` and `Error: http status: 404`.
**Why.** The cause nr-09s recorded, still unfixed: wasm-pack's cached ChromeDriver is 154.0.8037.57 and the installed Chrome is 153.0.8010.53.
**Cost.** About ten minutes. The leg passed (94 tests) with ChromeDriver 153 from Chrome for Testing, passed through `wasm-pack test --chromedriver <path>`.
**Prevent by.** The prevention nr-09s names: `scripts/gate-full.sh` passes a ChromeDriver that matches the installed Chrome, or fails with a clear version-mismatch message.
**Seen before.** nr-09s, nr-qoi.

## The Python gate legs failed in a fresh worktree: no `.venv`, and the main checkout's lacks dependencies

**What happened.** The worktree has no `.venv`, so `py()` fell back to system `python3`: 29 unittest errors from missing modules. The main checkout's `.venv` gave 28 errors (no `requests`/`textual`) and has no `mypy`.
**Why.** The cause nr-qoi recorded: `install_shell` creates no Python environment, and `py()` falls back silently.
**Cost.** About five minutes to build `.venv` in the worktree with `pip install --group scripts/pyproject.toml:test --group scripts/pyproject.toml:dev`.
**Prevent by.** The prevention nr-qoi names: extend `install_shell` to create `.venv`, or make `py()` fail with a clear message.
**Seen before.** nr-qoi.
