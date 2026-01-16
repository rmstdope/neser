# Sub-issue: Implement Mapper 206 (Namco 118/DxROM/MIMIC-1)

**Parent Issue:** #TBD (Missing Mappers Implementation Tracking)

## Overview
Mapper 206 (Namco 118, also known as DxROM or MIMIC-1) is a simplified MMC3 variant used in approximately 44 NES/Famicom games. It's the 7th most common mapper by game count (~1.8% of all games).

## Priority
**HIGH** - This is in the top 10 most-used mappers and would significantly improve game compatibility.

## Hardware Specifications
- **Board names:** DxROM, Namcot 118, MIMIC-1
- **PRG ROM:** Up to 128 KB
- **CHR ROM:** Up to 256 KB  
- **Bank switching:** Similar to MMC3 but simplified
- **IRQ support:** None (main difference from MMC3)
- **Mirroring:** Switchable H/V

## Implementation Details
Since neser already has MMC3 implemented (mapper.rs includes mmc3.rs), Mapper 206 can be implemented as a simplified variant:

1. Copy the MMC3 bank switching logic
2. Remove IRQ functionality (no scanline counter)
3. Adjust any MMC3-specific quirks that don't apply to Namco 118

## Documentation Resources
- **Primary:** [NESdev Wiki - INES Mapper 206](https://www.nesdev.org/wiki/INES_Mapper_206)
- **Hardware details:** [NESdev Wiki - Namco 118](https://www.nesdev.org/wiki/Namco_118)
- **Reference implementation:** Look at MMC3 implementation in `/home/runner/work/neser/neser/src/cartridge/mmc3.rs`

## Known Games Using This Mapper
- Various Namco titles
- Several Tengen games
- Dragon Spirit: The New Legend
- Digital Devil Story: Megami Tensei
- Babel no Tou
- Quinty (Mendel Palace)
- Many others (approximately 44 total)

## Technical Considerations
- Very similar to MMC3, so implementation should be straightforward
- Main difference is lack of IRQ support, which simplifies the implementation
- Should reuse MMC3 bank switching logic where possible
- Test with multiple games to ensure compatibility across different Namco board revisions

## Acceptance Criteria
- [ ] Mapper 206 case added to `create_mapper()` function in mapper.rs
- [ ] New file `namco118.rs` or `mapper206.rs` created in cartridge directory
- [ ] Basic bank switching working (PRG and CHR)
- [ ] Mirroring control implemented
- [ ] At least one test game loads and runs correctly
- [ ] No regressions in existing mapper tests
