# nr-f6o — retrospective

- **Implementer:** Nightcrawler
- **Date:** 2026-09-28
- **PR:** #3304

## Matching Mesen2's reset cycle count broke three blargg timing ROMs

**What happened.** Mesen2 runs 8 CPU cycles before the first opcode, and NESER ran 7. Making NESER's reset 8 cycles long moved the `$2002` reads onto Mesen2's dots. The next `cargo test --all-features --lib` then failed 17 tests, among them blargg's `4017_timing`, `cpu_interrupts_v2` `irq_and_dma` and `sprdma_and_dmc_dma`. The extra cycle had also moved things the CPU cycle count feeds. The APU saw one more clock during reset. OAM/DMC DMA get/put parity comes from `total_cycles`, and with 7 cycles NESER's numbering had silently cancelled out Mesen2's counter starting at -1.

**Why.** The CPU cycle count and the APU phase after reset are pinned by hardware test ROMs. Only the PPU's phase relative to the CPU was free to move. The fix kept the 7 cycles and gave the PPU the missing cycle's 3 dots instead (`power_on_ppu_lead`).

**Cost.** One full lib run (~3 min) and a rework of the fix. Before that, about an hour of 64-ROM sweeps against the 8-cycle variant, whose counts no longer applied.

**Prevent by.** For any change to power-on or reset timing, run `./scripts/test-dir.sh src/nes` (integration included) before sweeping games against Mesen2. The blargg ROMs in `nes::integration_tests::cpu_tests` and `apu_visual_tests` fail in seconds when a parity moves.

**Seen before.** nr-phv (a lag that started at power-on, from an extra reset).

## Mesen2's PAL power-on alignment contradicts a hardware test ROM's documentation

**What happened.** With the same 5-dot lead on PAL, blargg's nmi_sync `demo_pal.nes` matched Mesen2 pixel for pixel, yet `test_nmi_sync_demo_pal` failed. The middle line landed at x=81. The ROM's readme says the upper reference line (x=82) is "the farthest left it can ever be after reset".

**Why.** Mesen2 itself shows x=81 there, so its PAL alignment is one hardware does not produce, according to the ROM's author. The research skill ranks test ROMs above the implementation reference, so PAL and Dendy keep their 1-dot lead. Four PAL/Dendy games still differ from Mesen2 (nr-e1e).

**Cost.** About 20 minutes of region bisecting after the NTSC-only sweep counts moved unexpectedly. NESER's headless run reports three of the bead's "NTSC" games as PAL and one as Dendy.

**Prevent by.** A Mesen2 comparison sweep should record the region NESER picked for each ROM (the `Hardware:` line) next to its result, so region-gated changes are read per region. `scripts/reference_capture/README.md`, *Mesen2*, is the place to say so.

**Seen before.** none found.

## A latent MMC5 overflow appeared only under the new CPU/PPU phase

**What happened.** `test_mv_m005_0_write_protect` panicked with "attempt to add with overflow" in `mmc5.rs`: `(split_tile_count + 2) % 34` in `u8`, once the saturating count reached 254.

**Why.** Under the old phase the verification ROM's nametable reads never ran long enough without a scanline reset. The bug was there all along.

**Cost.** One investigation; the fix and a unit test ride on this PR.

**Prevent by.** Saturating counters used in arithmetic should be widened at the use site. `clippy::arithmetic_side_effects` would flag this class, but enabling it crate-wide is the navigator's call.

**Seen before.** none found.
