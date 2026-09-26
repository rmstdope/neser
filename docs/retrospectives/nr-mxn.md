# nr-mxn: retrospective

- **Implementer:** Bishop
- **Date:** 2026-09-27
- **PR:** #3217

## The bead's own fix (`startFrame`) was still one frame behind under load

**What happened.** The bead named the fix: capture on `startFrame` instead of `endFrame`.
It passed the new reproduction test and the gate. The cold review then measured 5 of 48
and 2 of 48 captures still at frame N-1 at load average 30-35, and I reproduced 4 of 48
under 40 busy loops. `emu.takeScreenshot()` copies the output of Mesen2's video-decode
thread, and `SendFrame()` only signals that thread, so a `startFrame` screenshot depends
on the decode having run in the few scanlines of vblank that remain. The fix became
`emu.getScreenBuffer()`, which runs synchronously on the emulation thread. It returns
pixels, not a PNG, so the script now carries its own encoder and a crop that matches
NESER's output.
**Why.** The bead's evidence (Mega Man X2 frames 817 and 830, and my first checks) was
gathered at ordinary load, where the race almost never loses. One capture per ROM cannot
show an intermittent miss, and nobody had read `emu.takeScreenshot()`'s path in Mesen2's
source before this review.
**Cost.** A second round: about an hour to reproduce the race, rewrite the script, find
the SNES crop offset, and re-verify under load, plus a delta review.
**Prevent by.** `scripts/reference_capture/README.md`, "Frame numbering" under Mesen2,
and the note in `snes-hardware-research` now name both off-by-one causes. The alignment
test samples five frames per ROM. Any reference-capture change is validated under
artificial CPU load (40 busy loops on this 18-core machine) as well as idle, because a
race only shows under load.
**Seen before.** nr-ve3 found the `endFrame` half of this. Nothing before it mentions the
decode thread.
