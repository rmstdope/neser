# nr-hab.1 — retrospective

- **Implementer:** Wolverine
- **Date:** 2026-09-26
- **PR:** #3205

## A throwaway frame probe made every Super FX ROM look hung

**What happened.** The first settle probe for the 31 PeterLemon GSU ROMs looped `snes.run_tick()` and counted a frame each time `snes.is_ready_to_render()` was true. Every ROM "settled" at frame 900, and the screen was black with one blue line. Tracing the S-CPU showed it moving through its init code at about 30 bytes per 10 "frames", which looked like something stealing huge amounts of time per instruction. Re-running with the chipset byte patched to plain LoROM gave the same crawl, which cleared the new GSU code.

**Why.** `is_ready_to_render()` stays true until `clear_ready_to_render()` is called, so the probe counted every tick after the first vblank as a frame. `rom_runner` clears the flag each frame; the probe did not.

**Cost.** About twenty minutes and three rebuilds, spent on a hang in the S-CPU that did not exist.

**Prevent by.** The "Probing animated ROMs with a per-frame CRC sweep" recipe in `snes-hardware-research` (which is not tracked in this repository) should say "on each `is_ready_to_render()` frame, call `clear_ready_to_render()`", or tell probes to reuse `rom_runner::run_rom_with_oracle` instead of their own loop.

**Seen before.** None found.

## wasm-pack test failed locally with "http status: 404" from ChromeDriver, again

**What happened.** The `wasm-pack test --headless --chrome` leg of `./scripts/gate-full.sh` failed before any test ran, on all three gate runs.

**Why.** The same cause as before: the cached ChromeDriver is 154.0.8037.57 and the installed Chrome is 153.0.8010.53.

**Cost.** About fifteen minutes. The leg passed (94, then 98 tests) with ChromeDriver 153.0.8010.53 from Chrome for Testing, passed through `--chromedriver`.

**Prevent by.** The prevention nr-09s and nr-630 name: update Chrome or clear `~/Library/Caches/.wasm-pack/chromedriver-*` on the fleet machine, or have `scripts/gate-full.sh` pass a matching `--chromedriver`.

**Seen before.** nr-09s, nr-qoi, nr-nr7, nr-6e9, nr-aph, nr-273, nr-ps1, nr-630.

## The gate's Python legs cannot run in a prepared worktree: no `.venv`

**What happened.** The worktree had no `.venv`, so `py()` fell back to system `python3` (`No module named ruff`). The main checkout's `.venv` gave 28 unittest import errors and has no mypy.

**Why.** The `install` declaration runs only `npm ci`.

**Cost.** About five minutes to build `.venv` in the worktree with both `scripts/pyproject.toml` dependency groups.

**Prevent by.** The prevention nr-qoi names: extend `install` in `.cerebro/project.conf` to create `.venv` with both dependency groups.

**Seen before.** nr-qoi, nr-nr7, nr-6e9, nr-aph, nr-273, nr-ps1, nr-630.

## The first gate failed `cargo fmt --check` because the commit hook never ran

**What happened.** `git config core.hooksPath` printed nothing in the prepared worktree, so `.githooks/pre-commit` never formatted the staged Rust, and the fast gate failed on formatting.

**Why.** The prepared worktree does not set `core.hooksPath`, although CLAUDE.md says it points at `.githooks`.

**Cost.** One extra fast-gate run and a formatting commit.

**Prevent by.** The prevention nr-6e9 names: have `scripts/prepare-worktree` (or `install`) run `git config core.hooksPath .githooks`.

**Seen before.** nr-6e9.
