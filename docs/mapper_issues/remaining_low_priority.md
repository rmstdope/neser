# Sub-issue: Implement Mappers 13, 15, 34, 78 (Lower Priority)

**Parent Issue:** #TBD (Missing Mappers Implementation Tracking)

## Overview
This issue covers four lower-priority mappers that have limited game usage but would improve overall compatibility.

## Priority
**LOW** - Few games per mapper, mostly for completeness.

---

## Mapper 13 (CPROM)

### Specifications
- **PRG ROM:** 32 KB (fixed)
- **CHR RAM:** 16 KB with 4 KB bank switching
- **Known games:** Videomation (only commercial game)

### Documentation
- [NESdev Wiki - CPROM](https://www.nesdev.org/wiki/CPROM)

### Implementation Notes
- Simple CHR-RAM bank switching
- Fixed PRG ROM
- Very straightforward implementation

---

## Mapper 15 (100-in-1 Contra Function)

### Specifications
- **Pirate multicart mapper**
- Various banking modes
- Unofficial hardware

### Documentation
- [NESdev Wiki - INES Mapper 015](https://www.nesdev.org/wiki/INES_Mapper_015)

### Implementation Notes
- Pirate/unofficial multicarts
- Lowest priority
- Multiple banking modes for different games on cart

---

## Mapper 34 (BNROM/NINA-001)

### Specifications
- **Two different hardware types:**
  - BNROM: Simple PRG switching
  - NINA-001: PRG + CHR switching
- **Known games:** Deadly Towers (NINA-001), several others

### Documentation
- [NESdev Wiki - INES Mapper 034](https://www.nesdev.org/wiki/INES_Mapper_034)

### Implementation Notes
- Need to handle two different board types
- Detection based on CHR ROM size or NES 2.0 header
- Relatively simple implementation

---

## Mapper 78 (NINA-03/NINA-06)

### Specifications
- **PRG ROM:** Up to 128 KB
- **CHR ROM:** Up to 128 KB
- **Bank switching:** 16 KB PRG, 8 KB CHR
- **Known games:** Various Tengen games (Pac-Man, RBI Baseball, Tetris)

### Documentation
- [NESdev Wiki - INES Mapper 078](https://www.nesdev.org/wiki/INES_Mapper_078)

### Implementation Notes
- Used by Tengen for unlicensed releases
- Simple bank switching
- Mirroring control

---

## Implementation Strategy

These mappers can be implemented individually as time/interest permits:

1. **Mapper 34** - Most useful (Deadly Towers)
2. **Mapper 78** - Several Tengen games
3. **Mapper 13** - Only one game but interesting
4. **Mapper 15** - Lowest priority (pirate multicarts)

## Acceptance Criteria
For each mapper implemented:
- [ ] Mapper case added to `create_mapper()` in mapper.rs
- [ ] New implementation file created
- [ ] Bank switching implemented correctly
- [ ] At least one test game loads
- [ ] No regressions in existing mappers

## Notes
These mappers can be tackled in separate PRs or as a single PR implementing multiple simple mappers together.
