# nr-e1e — retrospective

- **Implementer:** Shadowcat
- **Date:** 2026-09-29
- **PR:** #3321

## A "Mesen2 or the test ROM" decision came from a sweep that was too coarse

**What happened.** The bead asked the navigator to choose between Mesen2's PAL power-on alignment and blargg's nmi_sync `demo_pal` readme, since no PPU lead seemed to satisfy both. The sweep behind that claim moved `power_on_ppu_lead` in whole dots only. Sweeping in master-clock ticks instead (a temporary override of the MasterClock start, 5 ticks per PAL dot) found a band, 28..30 ticks, where the games, a 16-program vblank-poll sweep and `demo_pal` all agree with Mesen2. A 6-dot lead is 30 ticks.

**Why.** A PAL CPU cycle is 3.2 dots, so the phases that matter fall between whole dots. Mesen2's own phase is about one tick past the band, and `demo_pal` catches exactly that tick.

**Cost.** None to this bead, which went straight to the finer sweep. The earlier bead, nr-f6o, spent a navigator question on a choice that did not exist.

**Prevent by.** For a CPU/PPU alignment question on PAL (or any region whose dividers are not a multiple of each other), sweep the MasterClock start in ticks as well as the PPU lead in dots before declaring a conflict. The proposed place for that note is `scripts/reference_capture/README.md`, *Mesen2*, next to nr-f6o's proposed note on recording the region per ROM. This file only proposes it; the change is the navigator's.

**Seen before.** nr-f6o (the same conflict, declared from the whole-dot sweep).

## Dendy's vblank races were pinned to NTSC's scanline

**What happened.** No Dendy lead matched Mesen2 on the 16 vblank-poll programs. Mesen2 misses the first vblank at 6 and 15 NOPs of padding, and NESER missed it at 0 and 9. `Ppu::get_status` and `Ppu::write_control` tested `scanline == 241` for the $2002 suppression races and the NMI-disable race. On a Dendy, VBlank starts at 291 and 241 is post-render. A read at 291/0 never suppressed, and a poll crossing 241/0-1 suppressed the real VBlank at 291.

**Why.** Dendy's `vblank_start_scanline` arrived after these races were written. `tick.rs` was moved onto the region field, but these three literals in `ppu.rs` were not.

**Cost.** One extra sweep to tell it apart from an alignment problem.

**Prevent by.** A region-parameterised unit test beside each region-dependent PPU race, as `test_*_in_every_region` in `src/nes/ppu/ppu.rs` now does. Grep for literal `241` and `261` in `src/nes/ppu/` when a region field is added.

**Seen before.** none found.
