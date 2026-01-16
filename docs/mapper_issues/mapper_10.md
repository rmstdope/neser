# Sub-issue: Implement Mapper 10 (MMC4/FxROM)

**Parent Issue:** #TBD (Missing Mappers Implementation Tracking)

## Overview
Mapper 10 (MMC4/FxROM) is used exclusively in 3 high-profile Famicom strategy games. Despite the low game count, these are important titles that would benefit from emulator support.

## Priority
**MEDIUM-HIGH** - Only 3 games, but they're significant titles (Fire Emblem series, Famicom Wars).

## Hardware Specifications
- **Board names:** FxROM (MMC4)
- **PRG ROM:** Up to 256 KB
- **CHR ROM:** Up to 128 KB
- **PRG RAM:** 8 KB (battery-backed)
- **Bank switching:** 16 KB PRG banks, latch-based CHR banking
- **Mirroring:** Switchable H/V
- **Special feature:** CHR banking triggered by PPU reads (tile latches)

## Implementation Details
MMC4 is very similar to MMC2 (Mapper 9), which is already implemented in neser:

1. **Review existing MMC2 implementation:**
   - Located at `/home/runner/work/neser/neser/src/cartridge/mmc2.rs`
   - MMC4 uses the same latch-based CHR banking mechanism

2. **Key differences from MMC2:**
   - Larger PRG ROM support (256 KB vs 128 KB)
   - Larger CHR ROM support (128 KB vs 128 KB)
   - Slightly different register addresses
   - PRG banking configuration

3. **Implementation strategy:**
   - Can likely reuse most of MMC2 code
   - Adjust register mapping
   - Modify PRG bank sizes/configuration

## Documentation Resources
- **Primary:** [NESdev Wiki - MMC4](https://www.nesdev.org/wiki/MMC4)
- **Comparison:** [NESdev Wiki - MMC2](https://www.nesdev.org/wiki/MMC2) (already implemented)
- **Reference implementation:** `/home/runner/work/neser/neser/src/cartridge/mmc2.rs`

## Known Games Using This Mapper
**Only 3 games, all Famicom exclusives:**
1. **Fire Emblem: Ankoku Ryū to Hikari no Tsurugi** (Fire Emblem: Shadow Dragon and the Blade of Light)
   - First game in the Fire Emblem series
   - Important Japanese tactical RPG

2. **Fire Emblem Gaiden**
   - Second Fire Emblem game
   - Unique mechanics in the series

3. **Famicom Wars**
   - Turn-based strategy game
   - Part of the Advance Wars lineage

## Technical Considerations
- Very similar to MMC2 (already implemented)
- Latch-based CHR banking is the unique feature (shared with MMC2)
- These games have large save data requirements (8 KB PRG-RAM)
- All three games are Japanese Famicom exclusives
- Will need appropriate test ROMs or the actual game ROMs for validation

## Acceptance Criteria
- [ ] Mapper 10 case added to `create_mapper()` function in mapper.rs
- [ ] New file `mmc4.rs` created in cartridge directory (or extend mmc2.rs)
- [ ] PRG bank switching implemented
- [ ] Latch-based CHR bank switching implemented (similar to MMC2)
- [ ] PRG-RAM support with proper sizing
- [ ] Mirroring control implemented
- [ ] At least one test game (Fire Emblem or Famicom Wars) loads correctly
- [ ] No regressions in existing mapper tests
