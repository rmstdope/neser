# nr-nwy — retrospective

- **Implementer:** Shadowcat
- **Date:** 2026-09-29
- **PR:** #3323

## A "NESER runs the wrong mapper" sweep finding was Mesen2 missing its own database

**What happened.** The bead listed doraemon-world-3-doraemon-hack.nes as "header says mapper 120; NESER runs it as mapper 66 and stays on the title, Mesen2 (mapper 120) is in gameplay". `Mesen --testRunner --enableStdout` with a Lua script that only calls `emu.stop` printed `PRG+CHR CRC32: 0x588E15A3` and `[DB] Game not found in database`. NESER's PRG+CHR CRC is BDE3AE9B, and that CRC is the licensed GNROM Doraemon (mapper 66) in both NESER's `rom_db.csv` and Mesen2's `MesenNesDB.txt`. The file has 512 bytes after the CHR ("Doraemon  \r\n" and zeros), and Mesen2 includes them in its PRG+CHR CRC. With the trailer trimmed, Mesen2 runs mapper 66 and matches NESER at 10 of 12 checkpoints. The other two are a one-frame timing lead.
**Why.** Mesen2 hashes everything after the header as "PRG+CHR", so any trailing data defeats its database lookup and it falls back to the header, which is wrong for this file. NESER hashes the header-sized PRG+CHR and also tries the whole payload.
**Cost.** About 20 minutes of investigation on a ROM with nothing to fix in NESER.
**Prevent by.** In the sweep and rerun recipe (the command block in sweep-derived beads, and `scripts/reference_capture/README.md`), run Mesen2 with `--enableStdout` once per ROM and keep its `[iNes]`/`[DB]` lines. If Mesen2 says `Game not found in database` or `File is larger than expected`, compare its mapper with NESER's `Loaded rom with ... mapper=` line before filing the difference as a NESER bug.
**Seen before.** nr-1le (Mesen2's game database, not the hardware, caused a reference difference).
