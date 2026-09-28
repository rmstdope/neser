# nr-0an: retrospective

- **Implementer:** Bishop
- **Date:** 2026-09-28
- **PR:** #3302

## A "timing drift" came from Mesen2's local controller settings

**What happened.** Super Bomberman 3 differed from Mesen2 at frame 3000 only. A per-frame
comparison from 2600 to 3320 showed Mesen2 repeating frame 2916 (a lag frame) and NESER
staying a frame ahead until 3252. That looked like NESER's CPU running the heavy frame
about 8% too fast. An instruction trace aligned clock for clock showed instead a branch
going the other way: the game's per-player loop at `$C000A9` saw player 2 connected in NESER
and not in Mesen2. The local Mesen2 `settings.json` had `"Port2": "None"`, while NESER puts a
standard pad in each port. With `--snes.port2.type=SnesController` every checkpoint matched.
**Why.** The documented Mesen2 SNES flags pinned frame skipping and power-on RAM, but not
the controller ports, so testRunner took them from the machine's settings file.
**Cost.** About an hour: a per-frame dump, two trace rounds and one misaligned diff before
the branch showed up. The same command sits in 19 other open sweep beads.
**Prevent by.** The port flags are now in `scripts/reference_capture/README.md`, in
`SNES_FLAGS` in `scripts/test_mesen2_capture.py` (pinned by `TestMesen2SnesFlags`), and in the
`snes-hardware-research` checklist ("Plug in the same controllers on BOTH sides"). The sibling
beads carry a note. Also, with a lag-frame shape (one side repeats a frame), look for a
**branch** divergence in a lockstep trace before measuring cycle costs.
**Seen before.** The same class of confound as #3063's random power-on RAM
(`snes-hardware-research`, "Pin the power-on RAM state on BOTH sides"). No retrospective
names the controller ports.
