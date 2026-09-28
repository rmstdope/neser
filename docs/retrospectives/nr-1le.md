# nr-1le — Vs. System ROMs are shown with the Vs. palette

## Mesen2 draws some Vs. games with the wrong palette, and I nearly copied it

**What happened.** Vs. Duck Hunt differed from Mesen2 in colour. NESER's `rom_db.csv` holds the same
Vs. PPU values as Mesen2's `MesenNesDB.txt`, and Mesen2 reads its value as its own `PpuModel` enum
(0 = 2C02). So I concluded that NESER's column was Mesen2's numbering, and that NESER misread it as
the NES 2.0 nibble (0 = RP2C03). The first commit "fixed" that. The review found that
`rom_db.csv` is generated from nes20db (`scripts/nes_rom_db_scraper`), in NES 2.0 numbering, and
that the bug is in Mesen2's `GameDatabase.cpp:61`, which casts the nibble straight into `PpuModel`.
Duck Hunt's real board has an RP2C03, which NESER already drew.

**Why.** The values in the two databases were identical, so I took Mesen2 as their source without
checking the generator. The bead's framing ("Mesen uses the Vs. palette, NESER the standard") was
backwards against the fresh rerun, and I followed Mesen2 as the reference instead of checking the
specification first.

**Cost.** One wrong commit, a blocking review finding, a revert, a second full gate and a delta
review. Had it merged, three RC2C05 games (Mighty Bomb Jack, Gumshoe, Stroke & Match Golf) would
have got the wrong `$2002` security byte or register swap.

**Prevent by.** An entry in `.cerebro/traps.md`: "Mesen2 is not the reference for a Vs. System
game's PPU. `GameDatabase.cpp` casts the NES 2.0 Vs. PPU nibble into `PpuModel`, so DB values 0, 1
and 6–11 give the wrong palette or registers. Our `rom_db.csv` comes from nes20db, in NES 2.0
numbering." And a line in `nes-hardware-research`'s Mesen2 caveats saying the same.

**Seen before.** No.
