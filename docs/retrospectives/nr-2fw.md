# nr-2fw: retrospective

- **Implementer:** Bishop
- **Date:** 2026-09-28
- **PR:** see the bead

## A two-frame "attract pace" difference was Mesen2 shortening a PAL scanline

**What happened.** Metal Combat (the PAL release, region byte `$02`) ran two frames behind
Mesen2 from frame 252 on. Per-frame screen hashes matched exactly up to 251, and every
early frame was either static or black, so none of them showed where it started. NMI-entry
stamps with a WRAM hash put the first difference at NMI 4, and an instruction trace aligned
clock for clock showed Mesen2 taking the NMI one instruction earlier. Line-start clocks from
both traces showed why: Mesen2's scanline 240 of that frame was 1360 clocks and NESER's
1364. Mesen2 applies the short line at 50 Hz; fullsnes and ares make it NTSC-only. With
the rule copied locally, all 12 checkpoints matched Mesen2 at 0 px, so NESER was right and
the whole difference came from the reference.
Along the way, the sweep's Mesen2 command had a pad in port 2 where NESER auto-selects a
Super Scope for this game. That changed four WRAM bytes but no frame.
**Why.** The sweep treated every Mesen2 difference as a NESER suspect. Nothing in the
comparison recipe mentions region or which frame-length rules the two emulators disagree
on, and the per-game device lists were not part of the recipe either.
**Cost.** About an hour of per-frame hashing and tracing before the line lengths were
compared.
**Prevent by.** `snes-hardware-research`, "A PAL game drifts against Mesen2 by design"
(added in this PR): check the region first, and confirm with the local one-line change
before tracing. The same PR adds the pointer to the Super Scope and mouse lists under
"Plug in the same controllers". `short_scanline_240_is_ntsc_only` keeps anyone from
"fixing" NESER to match Mesen2.
**Seen before.** nr-0an (a Mesen2 settings difference that looked like CPU drift). Nothing
on PAL (`grep -rli "short scanline\|scanline 240" docs/retrospectives/` is empty).
