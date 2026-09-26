# nr-ve3 — retrospective

- **Implementer:** Bishop
- **Date:** 2026-09-26
- **PR:** #3212

## The committed Mesen2 capture script is one frame behind, and it read as an emulator bug

**What happened.** The bead reported Mega Man X2 and X3 running "exactly one frame ahead of
Mesen2 after a scene change". The first hour went on looking for that frame in NESER. Per-frame
WRAM, VRAM, CGRAM, OAM and brightness hashes at each frame end were identical on both sides for
frames 805-900, while the screenshots differed. `scripts/reference_capture/mesen2_capture.lua` calls
`emu.takeScreenshot()` from an `endFrame` callback. In Mesen2 that event fires before the PPU sends
the frame it has just rendered (`SnesPpu.cpp`: `ProcessEvent(EventType::EndFrame)`, then
`_frameCount++; SendFrame()`; `NesPpu.cpp:1299` uses the same order). So `CAPTURE_FRAME=N` saves
frame N-1. The offset only shows once the screen animates. The same script moved to `startFrame`
matched NESER's same-numbered frames at 0 px. The bead also held two real defects: MVN/MVP undercharging
every byte after the first, and the DMA registers powering up as `$00` instead of fullsnes' `FFh`.
**Why.** The recipe was verified on static frames only (`scripts/reference_capture/README.md`: the NES
check is frame 120 of a static ROM), and a static frame cannot show a one-frame offset.
`snes-hardware-research` says both "NESER matches Mesen2 pixel-exactly at equal frame numbers" and,
in its scripted-input template, captures on `startFrame`, so the two workflows disagree without
saying so.
**Cost.** About an hour of state dumps before the capture pipeline was suspected. A second bead
(nr-phv, SFA2) had been annotated with the same inflated lead.
**Prevent by.** nr-mxn: capture on `startFrame` in `mesen2_capture.lua`, re-verify the NES and SNES
recipes on animated content, and correct the README and skill text. Until then, the first step of
any "N frames ahead of Mesen2" investigation is to diff NESER N against capture N and N+1 before
touching the emulator.
**Seen before.** None found (`grep -rli "endFrame\|takeScreenshot" docs/retrospectives/` is empty),
though the skill's "verify the reference capture pipeline before debugging the emulator under test"
(#2990) is the same lesson from a different cause.

## The Python gate legs fail in a fresh worktree: no `.venv`

**What happened.** `./scripts/gate-full.sh` failed its unittest leg with 29 import errors
(`requests`, `rich`, `bs4`). The prepared worktree has no `.venv`, so `py()` fell back to the system
`python3`. The main checkout's `.venv` gives the same 28 errors and has no mypy.
**Why.** The `install` declaration is only `npm ci`, so nothing creates the venv.
**Cost.** About ten minutes to build a scratchpad venv with
`pip install --group scripts/pyproject.toml:test --group scripts/pyproject.toml:dev`, symlink it as
`.venv` for the gate, and remove the symlink before committing.
**Prevent by.** The prevention nr-qoi and nr-630 name: extend `install` in `.cerebro/project.conf` to
create `.venv` with both dependency groups, or make `py()` fail with a clear message.
**Seen before.** nr-qoi, nr-630, nr-hab.1.

## The wasm-pack leg failed with ChromeDriver "http status: 404"

**What happened.** The same failure as before: wasm-pack's cached ChromeDriver is 154, Chrome is
153. It passed (98 and 101 tests) with a ChromeDriver 153 first on `PATH`.
**Why.** As nr-09s recorded.
**Cost.** About ten minutes and one extra full gate run.
**Prevent by.** As nr-09s and nr-630 say.
**Seen before.** nr-09s, nr-6e9, nr-aph, nr-273, nr-630 and others.

## Every `bd` call in the fleet hung for about 13 minutes behind one `bd dolt push`

**What happened.** A `bd create` stalled; `lsof ~/repos/neser/.beads/embeddeddolt.gate.lock` showed
another session's `bd -C … dolt push` (started 22:06) holding the gate lock, with nine `bd`
processes (heartbeats, `epic status`, `show`, my `create`) queued behind it. It finished on its own at
about 22:19.
**Why.** Not established. A push waiting on the network while holding the embedded-Dolt gate lock is
the likely shape.
**Cost.** About fifteen minutes of blocked bead updates. Heartbeats queued too, so every live
session's lease was at risk.
**Prevent by.** A timeout on `bd dolt push`'s network step, or releasing the gate lock during the
remote transfer. Until then, `lsof <repo>/.beads/embeddeddolt.gate.lock` is the one-command
diagnosis for a hung `bd`.
**Seen before.** None found.
