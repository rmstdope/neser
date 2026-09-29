# nr-sjt — retrospective

- **Implementer:** Bishop
- **Date:** 2026-09-29
- **PR:** #3324

## Five of six "Mesen2 is blank" differences were not NESER rendering bugs

**What happened.** The bead listed six ROMs where Mesen2 showed a blank screen and NESER did not. Only Capcom 30-in-1 was a NESER bug (a palette write through $2007 during rendering). Of the rest: Dragon Quest 3 (hm02) runs as mapper 6 in Mesen2 because its game database matches the unhacked original's CRC. The Dragon Ball Z translation has a mapper-16 header on a mapper-159 (24C01) game, and it boots in NESER only because NESER's mapper 16 has no EEPROM. G.I. Joe and Power Rangers 2 differ only through the branch-delays-NMI rule NESER keeps from nr-046.
**Why.** The sweep compares against Mesen2 with its game database enabled. It also compares with no filter for differences that are already decided (nr-046) or that come from the dump.
**Cost.** About two hours of tracing (Lua exec and memory callbacks in Mesen2 against a debug build's `--trace-cpu`) to rule out four ROMs, plus one reverted experiment to prove the nr-046 attribution.
**Prevent by.** In `scripts/reference_capture/README.md`, adding `--nes.DisableGameDatabase=true` for ROMs whose header Mesen2's log shows being overridden (`[DB] Mapper:` differs from `[iNes] Mapper:`), and checking a timing difference against the nr-046 branch rule before filing it. The DBZ trap is now noted by the EEPROM limitation in `src/nes/cartridge/bandai/bandai_fcg.rs`.
**Seen before.** none found

## nesdev.org answered curl with a Cloudflare challenge page

**What happened.** `curl -sL https://www.nesdev.org/wiki/PPU_registers` returned a 5 KB "Just a moment..." page. WebFetch retrieved the same page.
**Why.** Cloudflare bot protection on nesdev.org, not established further.
**Cost.** One round trip. A script that trusted the file would have grepped an empty page and reported "not in the specification".
**Prevent by.** In `nes-hardware-research`'s retrieval order, checking the fetched page's `<title>` for "Just a moment" before using it, then falling back to WebFetch or the nes.science mirror.
**Seen before.** none found
