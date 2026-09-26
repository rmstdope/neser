# nr-1gg — retrospective

- **Implementer:** Storm
- **Date:** 2026-09-26
- **PR:** #3213

## A GameBoy unit test spun for seven minutes: a ROM of zeros never finishes a frame on the CGB bus

**What happened.** New `gameboy.rs` tests ran `run_tick()` until `is_frame_ready()` on the existing `minimal_cgb_rom()` helper, which is all zeros. The test binary ran at full CPU for over seven minutes until killed. `./scripts/test-dir.sh` has no timeout, and macOS has no `timeout` command.
**Why.** The CPU runs a sled of NOPs through ROM into other memory and meets a `STOP` (0x10), which on CGB stops the LCD, so no frame is ever ready. The existing tests only used a fixed number of ticks, so the helper never had to produce frames.
**Cost.** About ten minutes, including one blind rerun that printed nothing.
**Prevent by.** Test helpers that wait for frames use a ROM that idles at the entry point (`JR -2`, bytes `18 FE` at $0100), as `idling()` in `src/gb/console/gameboy.rs` tests now does. A frame loop in a test should also cap its ticks.
**Seen before.** none found.

## Playwright could not be run as documented: port 8000 held by another session, and no Playwright browser

**What happened.** `npm run test:integration:web` serves `dist/` on the fixed port 8000 with `reuseExistingServer: true`. Another session's server already held 8000, so the suite would have tested that session's build. Separately, the Playwright headless shell was not installed (`Executable doesn't exist at .../chromium_headless_shell-1208`). With my own server on a private port, a scratch config using `channel: "chrome"`, and the default workers, two tests failed with `ERR_CONNECTION_TIMED_OUT` against `python3 -m http.server`. `--workers=1` passed all six.
**Why.** `playwright.config.ts` hard-codes port 8000 and has no `port_base`/`smoke-port` hook (`.cerebro/project.conf` declares none). The browser was never installed on this machine. The timeouts under parallel workers were not established.
**Cost.** About fifteen minutes, three runs.
**Prevent by.** Declare `port_base`/`port_env` in `.cerebro/project.conf`, and have `playwright.config.ts` and `scripts/run_web.sh` read the port from that variable, so `smoke-port` gives each session its own. Use `channel: "chrome"` (or install the Playwright browser in the project's `install`).
**Seen before.** none found.

## wasm-pack test failed locally with "http status: 404" from ChromeDriver, again

**What happened.** The same failure nr-630 describes: wasm-pack's cached ChromeDriver is 154, and Chrome is 153. As nr-630 found, a ChromeDriver 153 first on `PATH` fixed it (downloaded from Chrome for Testing into the scratchpad).
**Why.** As nr-09s and nr-630 recorded.
**Cost.** About ten minutes.
**Prevent by.** As nr-630: update Chrome or remove the stale cached ChromeDriver on the fleet machine.
**Seen before.** nr-630, nr-09s, nr-6e9, nr-nr7, nr-aph, nr-273, nr-ps1, nr-qoi, nr-hab.1.

## The gate's Python legs failed in the fresh worktree (no `.venv`)

**What happened.** 29 unittest import errors from the system `python3` fallback, as before. A venv in the scratchpad built from `scripts/pyproject.toml`'s `test` and `dev` groups ran all 452 tests, plus ruff and mypy, green.
**Why.** As nr-qoi recorded.
**Cost.** About five minutes.
**Prevent by.** As nr-qoi: create `.venv` in the project's `install`.
**Seen before.** nr-6e9, nr-273, nr-aph, nr-ps1, nr-qoi, nr-hab.1, nr-630, nr-nr7.
