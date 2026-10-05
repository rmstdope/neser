# nr-i2x — retrospective

- **Implementer:** Bishop
- **Date:** 2026-10-05
- **PR:** #3346

## One symptom group held three causes, one of them the nr-046 difference a fourth time

**What happened.** The bead grouped three intros because they show the same symptom, "different
random stars". They had three different causes. Snake Rattle 'n Roll and Battletoads & Double
Dragon were the nr-046 NMI-in-a-taken-branch difference the navigator already decided to keep.
Super Spy Hunter had two real NESER bugs one after the other: a database row stating no PRG-RAM
was never applied to an iNES 1.0 header, which cannot say "none" (fixed here), and a mid-render $2007 read refilled from `v` (filed as nr-6wgi).
Neither real bug showed up in the NMI clock log. Both were found by logging every bus read of the
game's RNG instruction (`EOR ($EF),Y` at $D182) in both emulators and diffing the (address, value)
pairs. That needed a temporary `eprintln!` in `Cpu::read`, gated on the PC, and a Mesen2 Lua read
callback armed between two exec callbacks.
**Why.** A sweep bead groups ROMs by what the picture shows, not by cause. `.cerebro/traps.md` is
still empty, so nothing said "run the nr-046 experiment build first" before tracing. nr-3wg's
retrospective proposed that entry, and nr-8px's proposed the same check in the README.
**Cost.** About 20 minutes re-deriving nr-046 before I found nr-3wg's retrospective. The NMI log
for Super Spy Hunter was all nr-046 noise until I traced with Mesen2's rule patched in.
**Prevent by.** Two things. First, the `.cerebro/traps.md` entry nr-3wg proposed (Mesen2 takes an
NMI that a taken same-page branch delays; trace Mesen2-parity drift with that rule patched in
first). Second, a "per-instruction read log" recipe under "Tracing against Mesen2" in
`scripts/reference_capture/README.md`: a NESER read log gated on a PC, and the Mesen2 Lua read
callback armed by exec callbacks on the instruction and the next one. That is the step after the
exec trace when the instructions match but a value differs.
**Seen before.** nr-046, nr-3wg, nr-8px (the same branch/NMI difference).
