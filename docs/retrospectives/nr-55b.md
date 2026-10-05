# nr-55b — retrospective

- **Implementer:** Gambit
- **Date:** 2026-10-05
- **PR:** #3331

## Making the A12 detector unfiltered changed nothing: the mapper never saw A12 fall

**What happened.** The spec says J.Y. Company IRQ source 1 counts "PPU A12 rise (unfiltered, eight per scanline)", and the mapper used the MMC3 debounce (`A12RisingEdgeDetector::new(3)`). Setting the threshold to 0 left Final Fight 3's comparison against Mesen2 identical to the pixel. The reason: `Mapper::ppu_address_changed` is called only for CHR (pattern) fetches. The PPU never made, or reported, the two garbage nametable fetches of each sprite slot, so A12 stayed high across all eight sprite pattern fetches and only one rise per line was ever visible.
**Why.** Established. `ppu/memory.rs` notifies the mapper's address hook only from `read_chr`/`write_chr`, and `tick_sprites` makes no nametable fetches in dots 257-320. The name `ppu_address_changed` suggests the whole PPU bus.
**Cost.** One extra release build and a full Mesen2 comparison run (~10 min), plus a PPU-side change (the new `Mapper::ppu_nametable_address` hook) that the bead's mapper-only framing did not anticipate.
**Prevent by.** The `ppu_address_changed` doc comment in `src/nes/cartridge/mapper.rs` should say it carries CHR fetches only, and name `ppu_nametable_address` for nametable fetches. Any mapper that counts A12 edges without a filter (or counts PPU reads) must take both.
**Seen before.** none found
