# nr-9h6 — retrospective

- **Implementer:** Bishop
- **Date:** 2026-09-28
- **PR:** #3297

## A verification ROM encoded the same submapper swap as the mapper it was meant to check

**What happened.** While fixing CNROM's submapper-0 bus-conflict default, a check of the nesdev
NES 2.0 submappers page (sections 002/003/007) showed that NESER's UxROM (mapper 2) had
submappers 1 and 2 swapped: sub 2 ran without bus conflicts and sub 1 with them, the opposite of
the spec. The m002.2 verification ROM (`defs/m002.2_defs.inc`, `configs/m002.2.cfg`) was labelled
and built as "no bus conflicts" as well, so the implementation and its verification ROM agreed
with each other and both passed while disagreeing with the specification.

**Why.** Not established from history. The ROM's submapper meaning cannot have come from the
nesdev page, which says sub 2 = AND-type bus conflicts; it matches the implementation's comment
("Submapper 2 = explicitly no bus conflicts").

**Cost.** Six verification ROM variants rebuilt by a spec-only sub-agent, nine unit tests that
had used submapper 2 to mean "no conflicts" rewritten, and an extra review round on the ROMs.

**Prevent by.** `.github/skills/mapper-verification-roms/SKILL.md`, step 1: take submapper
meanings from the mapper's nesdev page and the NES 2.0 submappers page every time, never from an
existing ROM's defs, configs or header comments (added in this PR).

**Seen before.** None found.

## The bead's remaining symptom was a boot-timing lag, not the mapper

**What happened.** After the bus-conflict fix, checkpoints 2400-3600 of porno-island-hack.nes
still differed from Mesen2. NESER frame N+1 matched Mesen2 frame N exactly. A probe showed
NESER's first `LDA $2002 / BPL` vblank wait reading PPUSTATUS on the vblank-set edge and missing
frame 0's vblank, because power-on CPU/PPU alignment differs from Mesen2 by one CPU cycle (Mesen2
runs 8 reset cycles, NESER 7).

**Why.** Established by the probe; the numbers are in nr-mw5.

**Cost.** About an hour of frame-offset bisection and probing before the cause was visible, then
a split into nr-mw5.

**Prevent by.** When a Mesen2 comparison differs by a scroll or animation shift, compare NESER
frame N±1 against Mesen2 frame N before investigating rendering. A constant offset points at
timing, not the picture. `scripts/reference_capture/README.md` could name that step.

**Seen before.** nr-ve3 and nr-mxn: a one-frame offset against Mesen2 there came from the capture script. Here the script was already fixed, and the offset is real.
