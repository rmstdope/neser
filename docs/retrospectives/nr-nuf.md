# nr-nuf retrospective

## A single fixed "fresh" ROM name leaks Mesen2 battery saves between games

**What happened.** The bead's rerun recipe copies each ROM to `out/fresh.nes` "a fresh name: both
emulators load battery saves by ROM name". Run once that holds, but over the bead's 69 ROMs the
name is the same for every game: Mesen2 keeps saves in its own `Saves/<name>.sav` (not next to
the ROM), so the second battery-backed game would boot on the first one's SRAM. A `fresh.sav` was
already sitting in `~/Library/Application Support/Mesen2/Saves/` from an earlier run.

**Why.** The fresh-prefix workaround from nr-kds works because each ROM keeps a distinct name; a
constant name reintroduces the shared-save problem on the Mesen2 side. NESER is unaffected when
each ROM gets its own output directory, since it reads the `.sav` beside the ROM.

**Cost.** None in this bead: caught before the comparison ran, by giving each Mesen2 run a unique
file name. Undetected, battery games (Dragon Warrior, Crystalis, Zelda in this set) could have
shown spurious differences and been blamed on the fix.

**Prevent by.** The Mesen2 recipe in `scripts/reference_capture/README.md` and the reruns written
into sweep beads: copy each ROM to a name unique per ROM *and* per run (for example
`fresh-<rom>-<timestamp>.nes`), never a constant name.

**Seen before.** `docs/retrospectives/nr-kds.md` ("Both emulators load battery saves by ROM file
name") records the underlying fact and the fresh-prefix workaround.
