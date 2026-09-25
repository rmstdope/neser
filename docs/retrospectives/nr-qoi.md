# nr-qoi — retrospective

- **Implementer:** Cyclops
- **Date:** 2026-09-25
- **PR:** #3190

## wasm-pack test failed locally with "http status: 404" from ChromeDriver, again

**What happened.** On a documentation-only diff, the `wasm-pack test --headless --chrome` leg of `./scripts/gate-full.sh` failed twice with `driver status: signal: 9 (SIGKILL)` and `Error: http status: 404`, before any test ran.
**Why.** This is the cause nr-09s recorded, still unfixed. wasm-pack's cached ChromeDriver is 154.0.8037.57, and the installed Google Chrome is 153.0.8010.53.
**Cost.** About ten minutes. The leg passed (94 tests) with ChromeDriver 153.0.8010.53, downloaded from Chrome for Testing into the scratchpad and passed through `wasm-pack test --chromedriver <path>`.
**Prevent by.** The prevention nr-09s names: `scripts/gate-full.sh` passes `--chromedriver` pointing at a driver that matches the installed Chrome, or compares the two versions and fails with a clear message.
**Seen before.** nr-09s.

## The Python gate legs fail in a fresh worktree: no `.venv`, and the main checkout's is empty

**What happened.** After the wasm leg, the Python legs of the gate had nothing to run with. The prepared worktree has no `.venv`, so the gate's `py()` fell back to system `python3`: 29 unittest errors (missing modules), and `No module named ruff`. The main checkout's `.venv` holds only pip, so borrowing it failed the same way.
**Why.** `install_shell` in `.cerebro/project.conf` runs `git submodule update --init --recursive && npm ci` and creates no Python environment. `py()` silently falls back to the system interpreter instead of failing.
**Cost.** About five minutes: building `.venv` in the worktree with `python3 -m venv .venv` and `pip install --group scripts/pyproject.toml:test --group scripts/pyproject.toml:dev`, as CI does.
**Prevent by.** Extend `install_shell` in `.cerebro/project.conf` to create `.venv` and install the two `scripts/pyproject.toml` dependency groups. Alternatively, have `py()` in `scripts/gate-full.sh` fail with "no .venv: run …" instead of falling back to `python3`.
**Seen before.** None found.
