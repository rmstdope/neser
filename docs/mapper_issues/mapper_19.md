# Sub-issue: Implement Mapper 19 (Namco 163)

**Parent Issue:** #TBD (Missing Mappers Implementation Tracking)

## Overview
Mapper 19 (Namco 163) is a complex mapper with expansion audio used in approximately 20 NES/Famicom games, primarily Namco titles. It's the 10th most common mapper by game count.

## Priority
**HIGH** - Top 10 most-used mapper, important for Namco game compatibility.

## Hardware Specifications
- **Board names:** Namco 163, Namco 129
- **PRG ROM:** Up to 512 KB
- **CHR ROM:** Up to 512 KB
- **PRG RAM:** 8 KB internal, optional external
- **Expansion audio:** Up to 8 additional sound channels
- **Bank switching:** Very flexible, supports unusual configurations
- **IRQ support:** CPU cycle-based counter

## Implementation Details
This is a more complex mapper requiring:

1. **Advanced bank switching:**
   - 8 KB PRG banks (can map anywhere)
   - 1 KB CHR banks (8 separate banks)
   - Internal 8 KB RAM with special mapping

2. **Sound hardware (optional for initial implementation):**
   - 8 additional sound channels
   - Complex waveform synthesis
   - Can be implemented later for full compatibility

3. **IRQ system:**
   - 16-bit down counter
   - CPU cycle driven
   - Similar to other IRQ implementations in neser

## Documentation Resources
- **Primary:** [NESdev Wiki - INES Mapper 019](https://www.nesdev.org/wiki/INES_Mapper_019)
- **Hardware details:** [NESdev Wiki - Namco 163](https://www.nesdev.org/wiki/Namco_163)
- **Audio details:** [NESdev Wiki - Namco 163 audio](https://www.nesdev.org/wiki/Namco_163_audio)
- **Reference implementations:** FCEUX, Mesen, Nestopia sources

## Known Games Using This Mapper
- **Mappy Kids** (Famicom)
- **King of Kings** (Famicom)
- **Digital Devil Story: Megami Tensei II** (Famicom)
- **Famista '89** (Famicom)
- **Famista '90** (Famicom)
- **Battle Fleet** (Famicom)
- **Erika to Satoru no Yume Bouken** (Famicom)
- **Hydlide 3** (Famicom)
- **Mindseeker** (Famicom)
- **Rolling Thunder** (Famicom)
- **Sangokushi II** (Famicom)
- **Splatterhouse: Wanpaku Graffiti** (Famicom)
- **Wagyan Land** series
- Approximately 20 total games

## Technical Considerations
- Most complex mapper in the high-priority list
- Expansion audio can be implemented in a later phase
- Internal RAM mapping is unusual and requires careful handling
- IRQ system is relatively straightforward (similar to VRC6 which is already implemented)
- Test with games that don't use audio first, then add audio support

## Implementation Phases
**Phase 1 (Essential):**
- Bank switching (PRG and CHR)
- Internal RAM mapping
- IRQ counter
- Basic functionality to get games running

**Phase 2 (Enhancement):**
- Expansion audio channels
- Full audio feature parity with other expansion audio mappers (VRC6)

## Acceptance Criteria
- [ ] Mapper 19 case added to `create_mapper()` function in mapper.rs
- [ ] New file `namco163.rs` created in cartridge directory
- [ ] Bank switching implemented (PRG, CHR, internal RAM)
- [ ] IRQ counter implemented
- [ ] At least one test game loads and runs correctly (without audio)
- [ ] No regressions in existing mapper tests
- [ ] (Optional) Expansion audio channels implemented
