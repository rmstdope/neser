# nr-ps1 — retrospective

- **Implementer:** Cyclops
- **Date:** 2026-09-26
- **PR:** #3197

## wasm-pack test failed locally with "http status: 404" from ChromeDriver, a sixth time

**What happened.** The `wasm-pack test --headless --chrome` leg of `./scripts/gate-full.sh` failed before any test ran: `driver status: signal: 9 (SIGKILL)`, then `Error: http status: 404`. It failed the same way when re-run inside `smoke-port`.
**Why.** The cause nr-09s recorded, still present: wasm-pack's cached ChromeDriver is 154.0.8037.57, and the installed Chrome is 153.0.8010.53.
**Cost.** About fifteen minutes and two extra wasm builds. The leg passed (94 tests) with ChromeDriver 153.0.8010.53 from Chrome for Testing, passed through `wasm-pack test --chromedriver <path>`.
**Prevent by.** The prevention nr-09s names: `scripts/gate-full.sh` passes a `--chromedriver` that matches the installed Chrome, or fails with a clear version-mismatch message.
**Seen before.** nr-09s, nr-qoi, nr-6e9, nr-nr7, nr-aph, nr-273.

## The Python gate legs fail in a fresh worktree: no `.venv`

**What happened.** The prepared worktree has no `.venv`, so `py()` fell back to system `python3`, which has no ruff or mypy. The main checkout's `.venv` gave 28 unittest import errors and has no mypy.
**Why.** The cause nr-qoi recorded: the project's `install` (`npm ci`) creates no Python environment, and `py()` falls back silently.
**Cost.** About five minutes to build a scratchpad venv with `pip install --group scripts/pyproject.toml:test --group scripts/pyproject.toml:dev`. With it: 452 tests OK, and ruff and mypy clean.
**Prevent by.** The prevention nr-qoi names: extend the `install` declaration in `.cerebro/project.conf` to create `.venv` with both dependency groups, or make `py()` fail with a clear message.
**Seen before.** nr-qoi, nr-6e9, nr-nr7, nr-aph, nr-273.
