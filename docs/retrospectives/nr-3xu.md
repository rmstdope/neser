# nr-3xu — retrospective

- **Implementer:** Bishop
- **Date:** 2026-09-27
- **PR:** #3255

## Seven "stopped making progress" ROMs looked like seven mapper bugs, but shared one CPU cause

**What happened.** The bead listed seven ROMs on six different mappers where "NESER exits
with Error: Emulator stopped making progress during frame K where Mesen2 runs", and asked
for "one investigation per mapper". The jam traces showed seven unrelated-looking crash
paths. But all seven ended on a KIL opcode, and NESER's jammed CPU spent 0 cycles, which
froze the PPU. Mesen2 re-runs the jam opcode, so its PPU keeps making frames. Four of the
seven crash in Mesen2 as well (cattou's header even names the wrong mapper). After the fix
those four match Mesen2 at 0 px at all twelve checkpoints. Only three were real game bugs,
filed as nr-cl1, nr-2ao and nr-93z.
**Why.** "Mesen2 runs" was read from its captures continuing. A Mesen2 run whose game has
crashed still produces frames, so the sweep could not tell a running game from a jammed one.
**Cost.** About 30 minutes of per-mapper tracing before the common cause was clear.
**Prevent by.** The NES sweep that files these beads, or the bead template for "stopped
making progress", could say that this error means the CPU jammed or ticks stopped, not that
a mapper failed. It could also ask for the Mesen2 checkpoint images to be looked at first: a
black or frozen Mesen2 picture means Mesen2 crashed too.
**Seen before.** None found (`grep -rli "KIL\|stopped making progress" docs/retrospectives/`
is empty).

## Mesen2 capture printed nothing and timed out, because Lua I/O was off again

**What happened.** The bead's capture recipe, and a Lua probe script of my own, printed
nothing and ran until `--timeout`. I first read this as Mesen2 hanging on the jammed ROMs.
`settings.json` had `"AllowIoOsAccess": false`.
**Why.** Same as nr-1i3: the README tells you to enable it for the capture and then restore
it, so every session that follows the README leaves it off for the next one. The script gives
no error when I/O is disabled.
**Cost.** About 10 minutes, plus a wrong hypothesis (that Mesen2 hangs on a jam) that I
nearly followed.
**Prevent by.** The fix nr-1i3 proposed, still not made: `mesen2_capture.lua` should test the
`io.open` result, or `emu.getScriptDataFolder()`, and print an error and `emu.stop(1)` at
once. The bead template's capture recipe could also include the enable/restore lines from
`scripts/reference_capture/README.md`.
**Seen before.** nr-1i3.
