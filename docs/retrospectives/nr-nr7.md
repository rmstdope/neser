# nr-nr7 — retrospective

- **Implementer:** Cyclops
- **Date:** 2026-09-25
- **PR:** #3192

## The bead asked for a fix that was already on main

**What happened.** nr-nr7 (migrated from gh-3139) describes `SnesSystemBus::dma_tick` as never arming HDMA and quotes a `dma_tick` comment saying HDMA-during-DMA is unmodelled. On main, neither was true. PR #3171 (#3127, merged 2026-08-09) had already implemented the nesting through `DmaABus::take_due_hdma` and rewritten that comment. The bead was migrated on 2026-09-24 with its original text. What remained was the acceptance's real-bus unit tests, the "second GPDMA" test and the golden record, so the PR is tests only.
**Why.** gh-3139 was split out of #3083, and #3127 then fixed the same gap as a side effect without referencing #3139. So the issue stayed open, and the migration carried it over unchanged.
**Cost.** About ten minutes of reading before the plan could be written. The shape of the bead also changed: its title and acceptance describe a restructure, and the delivery is a set of regression tests.
**Prevent by.** When the migration (`docs/migration/github-to-beads.md`) or triage turns an issue into a bead, search `git log --grep` for the files and symbols the issue names and note any later PR on the bead. And a PR that fixes a gap tracked by another open issue names it in its body ("also fixes #N"), so the issue closes with it.
**Seen before.** None found.

## wasm-pack test failed locally with "http status: 404" from ChromeDriver, a third time

**What happened.** The `wasm-pack test --headless --chrome` leg of `./scripts/gate-full.sh` failed before any test ran, with `driver status: signal: 9 (SIGKILL)` and `Error: http status: 404`. A hand retry hung until killed.
**Why.** The same cause as before: the cached ChromeDriver is 154.0.8037.57 and the installed Chrome is 153.0.8010.53.
**Cost.** About fifteen minutes. The leg passed (94 tests) with ChromeDriver 153 from `npx @puppeteer/browsers install chromedriver@153` into the scratchpad, passed through `wasm-pack test --chromedriver <path>`.
**Prevent by.** The prevention nr-09s names: `scripts/gate-full.sh` passes a `--chromedriver` that matches the installed Chrome, or compares the two versions and fails with a clear message.
**Seen before.** nr-09s, nr-qoi.

## The Python gate legs fail in a fresh worktree, again

**What happened.** The worktree had no `.venv`. System `python3` gave 29 unittest errors and had no ruff or mypy. The main checkout's `.venv` had ruff but not mypy, and was missing test dependencies (28 errors).
**Why.** As nr-qoi records: `install_shell` creates no Python environment, and `py()` falls back silently to the system interpreter.
**Cost.** About five minutes. I built a venv in the scratchpad with `pip install --group scripts/pyproject.toml:test --group scripts/pyproject.toml:dev`, as CI does, and got ruff and mypy clean and 452 tests OK.
**Prevent by.** The prevention nr-qoi names: extend `install_shell` in `.cerebro/project.conf` to create `.venv` with both dependency groups, or have `py()` fail loudly.
**Seen before.** nr-qoi.
