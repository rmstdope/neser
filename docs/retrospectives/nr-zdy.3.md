# nr-zdy.3 — retrospective

- **Implementer:** Rogue
- **Date:** 2026-09-27
- **PR:** #3229

## A 535 KB SNES save state got into the PR through `git add -A`

**What happened.** `snes-f6-hotkey-test.state` appeared at the repo root after `cargo test` and went into a feature commit through `git add -A`. The cold review caught it (finding 1). It made up most of the diff.
**Why.** The existing test `snes_f6_save_state_returns_continue` in `src/frontends/native/keyboard/console_keyboard.rs` presses F6 on a console named `snes-f6-hotkey-test.sfc`, which writes `<name>.state` into the current directory, the repo root. `.gitignore` ignores only `test.state`.
**Cost.** One review round-trip and a removal commit.
**Prevent by.** Make that test write into a temporary directory, or add `*-hotkey-test.state` to `.gitignore`.
**Seen before.** none found.

## wasm-pack test failed locally with "http status: 404" from ChromeDriver, again

**What happened.** Same as nr-273: the cached ChromeDriver is 154 and the installed Chrome is 153. A 153 driver from Chrome for Testing, passed with `--chromedriver` or placed first on `PATH`, fixed it.
**Why.** As nr-09s and nr-273 recorded.
**Cost.** About fifteen minutes.
**Prevent by.** As nr-273: `scripts/gate-full.sh` uses a driver that matches the installed Chrome, or Chrome is updated on the fleet machine.
**Seen before.** nr-273, nr-1gg, nr-630, nr-09s, nr-aph.

## The gate's Python legs failed in the fresh worktree (no `.venv`)

**What happened.** 30 unittest import errors from the system `python3` fallback. The main checkout's `.venv` also lacks `requests`, `rich` and mypy. A scratchpad venv built from the `test` and `dev` groups in `scripts/pyproject.toml` ran green: 453 tests, ruff and mypy.
**Why.** As nr-qoi recorded: the project's `install` runs only `npm ci`.
**Cost.** About ten minutes.
**Prevent by.** As nr-qoi: create `.venv` in the project's `install`.
**Seen before.** nr-273, nr-1gg, nr-qoi, nr-630.
