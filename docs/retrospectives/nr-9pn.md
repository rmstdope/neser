# nr-9pn — retrospective

- **Implementer:** Bishop
- **Date:** 2026-10-05
- **PR:** #3343

## A bead filed as "sprite rows" was a background attribute bug

**What happened.** The bead's title and problem text suggested sprites ("a single sprite's row",
"sprite evaluation, sprite-overflow timing"). The Yo-Noid difference was a background tile in the
wrong palette. It only became clear once both emulators' `$2000-$2007` writes were logged per
scanline and dot. The `$2006` write at dot 200 landed between the attribute fetch and the
shift-register reload, and NESER chose the attribute quadrant at the reload.
**Why.** The sweep that filed the bead classified differences by shape (one row, 8 pixels). A
sprite row and a single background tile in another palette look the same at that level.
**Cost.** Little. Logging register writes found it within an hour, but reading sprite code first
would have been wasted.
**Prevent by.** For a one-scanline, 8-pixel difference at a split line, log the game's PPU register
writes on that line in both emulators first. Use a Mesen2 Lua `emu.addMemoryCallback` on writes to
$2000-$3FFF that records `ppu.scanline`/`ppu.cycle`, plus a matching temporary print in NESER's
`PpuDevice::write`. Then check whether a `$2006`/`$2005` write lands inside a tile fetch. Add that
first step to `scripts/reference_capture/README.md`, "Tracing against Mesen2".
**Seen before.** none found.

## Mesen2 comparisons time out or stall when the fleet loads the machine

**What happened.** `compare_mesen2` failed with "Mesen2 saved no screenshot" on frames 1200-2100 of
several ROMs, at a load average of about 230. With `--mesen2-arg=--timeout=300` it failed instead
with "another Mesen --testRunner is still running": other sessions' testRunners held Mesen2 for
over 300 s. A 17-ROM × 12-checkpoint sweep was not finishable.
**Why.** The capture command has a 30 s Mesen2 timeout, a 120 s subprocess cap and a 300 s wait for
other testRunners. Under fleet load all three are exceeded. Each failure aborts the remaining
frames of that ROM.
**Cost.** About two hours of a sweep that was abandoned. The regression check was redone without
Mesen2: both NESER builds were run with `--capture-every 300` and diffed against each other, and
only the changed frames were compared with Mesen2.
**Prevent by.** In `scripts/reference_capture/README.md`, document the NESER-against-NESER first pass
for regression sweeps: Mesen2 is only needed for frames whose pixels changed. Also consider making
`compare_mesen2` capture several frames per Mesen2 run, and continue past a failed frame.
**Seen before.** nr-1i3 (time lost to gate runs under load).
