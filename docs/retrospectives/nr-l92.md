# nr-l92 — retrospective

- **Implementer:** Rogue
- **Date:** 2026-09-27
- **PR:** #3250

## The bead's named game does not use the SNES Mouse

**What happened.** The bead was filed during nr-72o's verification. It said "SD Gundam GX starts, but needs an SNES Mouse to play", and its Outcome named that game. The ROM (`retro-scraper/roms/snes/sd-gundam-gx.sfc`, title `SD\xB6\xDE\xDD\xC0\xDE\xD1GX`) never reads a mouse's motion: it has no `$4016`/`$4017` serial reads and reads only `$4218`/`$421A`. A throwaway headless probe pressed Start and the d-pad: the menu shows an arrow pointer, and the d-pad moves it. Auto-connecting a mouse would have made the game unplayable. The navigator was asked in session and chose to auto-connect only mouse-only games (Mario Paint, Mario & Wario).
**Why.** Most likely the arrow pointer in its menu looks like a mouse cursor; the claim was never checked against the ROM before filing. Not established beyond that.
**Cost.** About 40 minutes of ROM inspection and probing, plus one question to the navigator during design. No rework, because it was caught before the build.
**Prevent by.** When a bead's premise is "game X needs peripheral Y", check the ROM for the peripheral before the bead is filed or the experience agreed. For the mouse: fullsnes "Detecting Controller Support of ROM-Images" (the `START OF MOUSE BIOS` string, or serial reads of `$4016`/`$4017`). One grep settles it. This check belongs in `write-bead` / `agree-experience` for peripheral beads.
**Seen before.** None found.

## The prepared worktree still had no `.venv`, though `install` now builds one

**What happened.** The first `wasm-pack test` run needed `scripts/chromedriver_match.py`, which runs under `.venv/bin/python`, and the tree had no `.venv`. `.cerebro/project.conf`'s `install_shell` already ends with `./scripts/setup-venv.sh`, the fix earlier retrospectives asked for. Running `./scripts/setup-venv.sh` by hand built it cleanly, and the gate then passed.
**Why.** Not established. Either this tree was prepared before `install_shell` gained `setup-venv.sh`, or the install step stopped before reaching it (it is the last command of an `&&` chain).
**Cost.** About five minutes.
**Prevent by.** Have `scripts/prepare-worktree` check that `.venv/bin/python` exists after `install` and refuse to hand over the tree otherwise, so a partial install is caught where it happens rather than at the gate.
**Seen before.** nr-qoi, nr-273, nr-630, nr-aph, nr-1gg, nr-zdy.3 (those saw an `install` without the venv step; this one has the step but not its result).
