# Sub-issue: Implement Mapper 69 (Sunsoft FME-7/5A/5B)

**Parent Issue:** #TBD (Missing Mappers Implementation Tracking)

## Overview
Mapper 69 (Sunsoft FME-7, also known as Sunsoft 5A/5B) is an advanced mapper with optional expansion audio. It's used in several notable games including the highly regarded Gimmick! and Batman: Return of the Joker.

## Priority
**MEDIUM-HIGH** - Important for high-quality Sunsoft titles, including games with impressive audio.

## Hardware Specifications
- **Board names:** FME-7 (Sunsoft 5A/5B)
- **PRG ROM:** Up to 512 KB
- **CHR ROM:** Up to 256 KB
- **PRG RAM:** Up to 8 KB
- **Bank switching:** 8 KB PRG banks, 1 KB CHR banks
- **IRQ support:** CPU cycle counter
- **Expansion audio:** 3 square wave channels + noise (5B variant only)
- **Mirroring:** Switchable H/V/1-screen

## Implementation Details
This mapper requires:

1. **Bank switching:**
   - 4 PRG ROM banks (8 KB each)
   - 8 CHR ROM banks (1 KB each)
   - 1 PRG RAM bank (8 KB)
   - Command/parameter register interface

2. **IRQ system:**
   - 16-bit down counter
   - CPU cycle driven
   - Enable/disable control
   - Similar to VRC6 IRQ (already implemented)

3. **Expansion audio (5B variant, optional):**
   - 3 square wave generators (similar to APU pulse channels)
   - 1 noise generator
   - Volume control per channel
   - Can be implemented later for full compatibility

## Documentation Resources
- **Primary:** [NESdev Wiki - Sunsoft FME-7](https://www.nesdev.org/wiki/Sunsoft_FME-7)
- **Audio details:** [NESdev Wiki - Sunsoft 5B audio](https://www.nesdev.org/wiki/Sunsoft_5B_audio)
- **Register reference:** Detailed register map on NESdev wiki
- **Reference implementations:** FCEUX, Mesen, Nestopia sources
- **Audio reference:** VRC6 audio in `/home/runner/work/neser/neser/src/cartridge/vrc6.rs`

## Known Games Using This Mapper
- **Gimmick! / Mr. Gimmick!**
  - Highly regarded platformer
  - Famous for advanced graphics and sound
  - Uses Sunsoft 5B expansion audio
  - Japanese and European releases

- **Batman: Return of the Joker**
  - High-quality action game
  - Smooth gameplay and graphics
  - European and Japanese releases

- **Hebereke** (Ufouria in the West)
  - Platform-adventure game
  - Japanese Famicom release

- **Maharaja**
  - Puzzle game
  - Japanese release

- Several other Sunsoft titles

## Technical Considerations
- Command/parameter interface is unusual (write command number, then parameter)
- IRQ system similar to VRC6 (already implemented in neser)
- Expansion audio is optional - can implement basic mapper first, then add audio
- The 5B sound chip is actually a Yamaha YM2149 variant
- Test with both audio and non-audio games

## Implementation Phases
**Phase 1 (Essential):**
- Bank switching (PRG, CHR, RAM)
- Mirroring control
- IRQ counter
- Basic functionality to get games running

**Phase 2 (Enhancement):**
- Expansion audio (5B variant)
- Full audio feature parity with VRC6

## Acceptance Criteria
- [ ] Mapper 69 case added to `create_mapper()` function in mapper.rs
- [ ] New file `sunsoft_fme7.rs` created in cartridge directory
- [ ] Command/parameter register interface implemented
- [ ] Bank switching implemented (PRG, CHR, RAM)
- [ ] Mirroring control implemented
- [ ] IRQ counter implemented
- [ ] At least one test game loads and runs correctly
- [ ] No regressions in existing mapper tests
- [ ] (Optional) Expansion audio channels implemented
