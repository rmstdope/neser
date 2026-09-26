# nr-72o — retrospective

- **Implementer:** Rogue
- **Date:** 2026-09-27
- **PR:** #3219

## Three sibling DSP beads were claimed at the same minute and all needed nr-608's shared rework

**What happened.** nr-608 (DSP-2, Wolverine), nr-72o (DSP-3, Rogue) and nr-tfq (DSP-4, Cyclops) were started within about a minute of each other. Only nr-608's acceptance defines the shared machinery all three needed: the genuine-dump check for every chip, the per-chip firmware table, and per-chip browser rows. I asked the navigator how to sequence. Meanwhile the three producers agreed over cross-session messages that nr-608 would own the machinery. By the time the navigator answered, nr-608 had merged. nr-tfq then merged while this PR was being opened. It edits the same lists (`FIRMWARE_FILES`, `SNES_FIRMWARE_CHIPS`, and the tests using "the one DSP chip that is not emulated yet" as their example), so rebasing gave conflicts in 14 files.
**Why.** The beads have no dependency on nr-608, although their acceptance says they "follow" nr-608's record. The fleet view therefore considered all three ready at once.
**Cost.** About twenty minutes spent waiting on the sequencing question. I aborted the rebase and re-applied the change on the new main; that took roughly half an hour, plus a second full gate run.
**Prevent by.** When a bead's acceptance defers to a sibling's still-open record ("follows everything agreed … on nr-608"), the planner or UX stage adds a `blocks` dependency on that sibling. Where several siblings add rows to one list, the second and later beads can also say in their design that they re-apply rather than rebase.
**Seen before.** None found.

## wasm-pack test failed locally with "http status: 404" from ChromeDriver, again

**What happened.** The `wasm-pack test --headless --chrome` gate leg failed twice before any test ran (`driver status: signal: 9`, `Error: http status: 404`). `CHROMEDRIVER=<path>` is overridden by wasm-pack. The leg passed (109, then 111 tests) with ChromeDriver 153.0.8010.53 from Chrome for Testing. I passed it through a `wasm-pack` wrapper on `PATH` that adds `--chromedriver <path>`.
**Why.** As nr-09s recorded: wasm-pack's cached ChromeDriver is 154.0.8037.57 and the installed Chrome is 153.0.8010.53.
**Cost.** About twenty minutes and two extra wasm builds.
**Prevent by.** As nr-09s: update Chrome, or remove the stale cached ChromeDriver on the fleet machine.
**Seen before.** nr-09s, nr-273, nr-1gg, nr-6e9, nr-630, nr-aph, nr-hab.1, nr-ps1, nr-nr7, nr-qoi, nr-ve3.

## The gate's Python leg failed with ModuleNotFoundError in a prepared worktree

**What happened.** A prepared worktree has no `.venv`, so the gate's `py()` falls back to `python3`, which has no `ruff`. The main checkout's `.venv` lacks `requests`, `bs4`, `rich`, `tqdm` and `mypy`, so 28 unittest modules errored. The leg passed (452 tests, ruff and mypy clean) in a fresh venv built in the scratchpad with `pip install --group scripts/pyproject.toml:test --group scripts/pyproject.toml:dev`. A `python3` symlink to that venv on `PATH` did not work: Python finds `pyvenv.cfg` next to the symlink, not next to its target.
**Why.** The project's `install` (`npm ci`) prepares no Python environment for a worktree.
**Cost.** About fifteen minutes.
**Prevent by.** Declare a Python install step in `.cerebro/project.conf` (a worktree `.venv` built from the `test` and `dev` groups), or have `scripts/gate-full.sh` say which venv it needs.
**Seen before.** Earlier files mention a venv; none found describing this failure.
