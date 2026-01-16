# Sub-issue: Implement Mappers 21-25 (VRC2/VRC4 variants)

**Parent Issue:** #TBD (Missing Mappers Implementation Tracking)

## Overview
Mappers 21, 22, 23, and 25 represent different pin configurations of Konami's VRC2 and VRC4 chips. These mappers are very similar to VRC6 (mappers 24/26), which is already implemented in neser. They're used in various Konami Famicom games.

## Priority
**MEDIUM** - Important for Konami Famicom game compatibility, leverages existing VRC6 implementation.

## Mapper Breakdown
- **Mapper 21:** VRC4a, VRC4c
- **Mapper 22:** VRC2a  
- **Mapper 23:** VRC2b, VRC4e (often treated as VRC4 in emulation)
- **Mapper 25:** VRC4b, VRC4d

## Hardware Specifications
- **PRG ROM:** Up to 512 KB
- **CHR ROM:** Up to 256 KB (VRC4), 128 KB (VRC2)
- **PRG RAM:** 8 KB
- **Bank switching:** 8 KB PRG banks, 1 KB CHR banks
- **IRQ support:** Yes (VRC4), No (VRC2)
- **Expansion audio:** None (unlike VRC6)
- **Mirroring:** Switchable H/V/1-screen

## Implementation Details
Since VRC6 is already implemented (`/home/runner/work/neser/neser/src/cartridge/vrc6.rs`), these mappers can reuse much of that code:

1. **Start with VRC6 implementation as base:**
   - Copy bank switching logic
   - Copy IRQ implementation (for VRC4)
   - Remove audio hardware

2. **Key differences from VRC6:**
   - No expansion audio channels
   - Different register address mappings (pin swizzling)
   - VRC2 has no IRQ (simpler than VRC4)

3. **Address line differences:**
   - Each mapper number represents different address line connections
   - The functionality is the same, just accessed at different addresses
   - NES 2.0 submappers can further specify exact board variants

## Documentation Resources
- **Primary:** [NESdev Wiki - VRC2 and VRC4](https://www.nesdev.org/wiki/VRC2_and_VRC4)
- **Individual mappers:**
  - [NESdev Wiki - INES Mapper 021](https://www.nesdev.org/wiki/INES_Mapper_021)
  - [NESdev Wiki - INES Mapper 022](https://www.nesdev.org/wiki/INES_Mapper_022)
  - [NESdev Wiki - INES Mapper 023](https://www.nesdev.org/wiki/INES_Mapper_023)
  - [NESdev Wiki - INES Mapper 025](https://www.nesdev.org/wiki/INES_Mapper_025)
- **NES 2.0 submappers:** [NESdev Wiki - NES 2.0 submappers](https://www.nesdev.org/wiki/NES_2.0_submappers)
- **Reference implementation:** `/home/runner/work/neser/neser/src/cartridge/vrc6.rs`

## Known Games Using These Mappers

### Mapper 21 (VRC4a/c) Games:
- **Wai Wai World 2** (Famicom)
- **Ganbare Goemon Gaiden** (Famicom)
- Various other Konami titles

### Mapper 22 (VRC2a) Games:
- **Contra** (Famicom version)
- **Crisis Force** (Famicom)
- **Twinbee 3** (Famicom)
- **Parodius da!** (Famicom)

### Mapper 23 (VRC2b/VRC4e) Games:
- **Gradius II** (Famicom)
- **Bio Miracle Bokutte Upa** (Famicom)
- **Tiny Toon Adventures** (Famicom)
- **Ganbare Goemon Gaiden 2** (Famicom)

### Mapper 25 (VRC4b/d) Games:
- **Gradius II** (different board variant)
- **Crisis Force** (Famicom)
- **Racer Mini Yonku** (Famicom)
- **Teenage Mutant Ninja Turtles** (Famicom)

## Technical Considerations
- Very similar to VRC6 implementation (already in neser)
- Main work is handling different address line mappings
- VRC2 vs VRC4 main difference is IRQ support
- Can potentially implement all 4 mappers in a single file with address translation
- VRC4 IRQ implementation can be copied from VRC6
- Test with multiple games across different mapper numbers

## Implementation Strategy
**Option 1: Single implementation file**
- Create `vrc2_vrc4.rs` that handles all variants
- Use mapper number to select address translation
- Share code between VRC2 and VRC4 where possible

**Option 2: Separate files**
- Create individual files for each mapper
- Share common code through a module
- More modular but potentially more code duplication

**Recommendation:** Option 1 (single file) - similar to how VRC6 handles mappers 24 and 26

## Acceptance Criteria
- [ ] Mapper 21, 22, 23, 25 cases added to `create_mapper()` function in mapper.rs
- [ ] New file `vrc2_vrc4.rs` (or similar) created in cartridge directory
- [ ] Bank switching implemented (PRG, CHR)
- [ ] Address line mapping handled for each variant
- [ ] IRQ counter implemented (VRC4 only: 21, 23, 25)
- [ ] Mirroring control implemented
- [ ] At least one test game per mapper loads and runs correctly
- [ ] No regressions in existing mapper tests (especially VRC6)
