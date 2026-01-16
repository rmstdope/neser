# Sub-issue: Implement Mapper 71 (Camerica/Codemasters)

**Parent Issue:** #TBD (Missing Mappers Implementation Tracking)

## Overview
Mapper 71 is used for Codemasters games published by Camerica for the NES, many of which were unlicensed. It's essentially a UNROM clone with mirroring control.

## Priority
**LOW-MEDIUM** - Unlicensed games, but includes some popular titles.

## Hardware Specifications
- **PRG ROM:** Up to 256 KB
- **CHR ROM:** None (uses CHR-RAM)
- **Bank switching:** 16 KB PRG banks (similar to UxROM/Mapper 2)
- **Mirroring:** Switchable 1-screen (main difference from UNROM)

## Implementation Details
Very similar to UxROM (Mapper 2, already implemented):
1. Review `/home/runner/work/neser/neser/src/cartridge/uxrom.rs`
2. Add 1-screen mirroring control
3. Adjust bank switching to match Mapper 71 register layout

## Documentation Resources
- **Primary:** [NESdev Wiki - INES Mapper 071](https://www.nesdev.org/wiki/INES_Mapper_071)
- **Reference:** UxROM implementation in neser

## Known Games
- Micro Machines
- The Fantastic Adventures of Dizzy
- Fire Hawk
- Bee 52
- MiG 29 - Soviet Fighter

## Acceptance Criteria
- [ ] Mapper 71 case added to `create_mapper()` in mapper.rs
- [ ] New file `camerica.rs` created
- [ ] PRG bank switching implemented
- [ ] 1-screen mirroring control implemented
- [ ] Test game loads correctly
