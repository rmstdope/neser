# Missing Mappers - Implementation Planning Summary

## Overview

This document provides a comprehensive analysis of NES mappers that are not yet implemented in neser, along with detailed issue templates ready to be created in GitHub.

## Current Status

**Implemented Mappers (13 types):**
- Mapper 0 (NROM)
- Mapper 1 (MMC1/SxROM) 
- Mapper 2 (UxROM)
- Mapper 3 (CNROM)
- Mapper 4 (MMC3/TxROM)
- Mapper 5 (MMC5/ExROM)
- Mapper 7 (AxROM)
- Mapper 9 (MMC2/PxROM)
- Mapper 11 (Color Dreams)
- Mapper 16 (Bandai FCG)
- Mapper 24/26 (VRC6a/b)
- Mapper 66 (GxROM/MxROM)

**Current Game Coverage:** ~85-88% of all NES/Famicom games

## Missing Mappers Identified

### High Priority (Top 10 Mappers by Usage)
1. **Mapper 206** (Namco 118/DxROM) - ~44 games
   - Simplified MMC3 variant
   - Can reuse existing MMC3 code
   - Relatively straightforward implementation

2. **Mapper 19** (Namco 163) - ~20 games  
   - Complex mapper with expansion audio
   - Important for Namco titles
   - Most complex high-priority mapper

### Medium-High Priority (Notable Games)
3. **Mapper 10** (MMC4/FxROM) - 3 key Famicom games
   - Fire Emblem series, Famicom Wars
   - Very similar to MMC2 (already implemented)
   - High impact despite low game count

4. **Mapper 69** (Sunsoft FME-7/5B) - Several notable games
   - Gimmick!, Batman: Return of the Joker
   - Advanced features including expansion audio
   - Important for high-quality Sunsoft titles

### Medium Priority (Konami VRC Variants)
5. **Mappers 21-25** (VRC2/VRC4 variants)
   - Various Konami Famicom games
   - Very similar to VRC6 (already implemented)
   - Can share significant code with VRC6

### Lower Priority
6. **Mapper 71** (Camerica/Codemasters)
   - Micro Machines, Dizzy series
   - Unlicensed games
   - Simple UNROM variant

7. **Mappers 13, 15, 34, 78** (Various)
   - Limited game counts (1-10 games each)
   - Implementation for completeness
   - Can be combined in a single PR

## Impact of Implementation

| Milestone | Mappers Added | Estimated Coverage |
|-----------|---------------|-------------------|
| Current | 13 types | 85-88% |
| After High Priority | +2 (206, 19) | ~90% |
| After Medium-High | +2 (10, 69) | ~92% |
| After Medium | +4 (21-25) | ~94% |
| After All | +5 (71, 13, 15, 34, 78) | ~95%+ |

## Documentation Created

### Location: `docs/mapper_issues/`

All issue templates are ready to be created as GitHub issues:

1. **PARENT_ISSUE.md** - Master tracking issue
2. **mapper_206.md** - Mapper 206 (Namco 118)
3. **mapper_19.md** - Mapper 19 (Namco 163)
4. **mapper_10.md** - Mapper 10 (MMC4)
5. **mapper_69.md** - Mapper 69 (Sunsoft FME-7)
6. **mappers_21_25.md** - Mappers 21-25 (VRC2/VRC4)
7. **mapper_71.md** - Mapper 71 (Camerica)
8. **remaining_low_priority.md** - Mappers 13, 15, 34, 78
9. **README.md** - Overview and usage instructions

### Each Template Contains:

- **Priority level** with justification
- **Hardware specifications** (PRG/CHR ROM sizes, features)
- **Implementation details** with specific guidance
- **Documentation resources** with direct NESdev Wiki links
- **Known games** that use the mapper
- **Technical considerations** specific to neser
- **Acceptance criteria** for implementation

## Next Steps

Since the agent cannot create GitHub issues directly, the repository owner/maintainer should:

### 1. Review the Templates
All templates are in `docs/mapper_issues/` and ready for use.

### 2. Create GitHub Issues

**Option A: Create all issues at once**
```bash
# Review and create issues from templates
# Start with PARENT_ISSUE.md
# Then create sub-issues for each mapper
```

**Option B: Create issues incrementally**
- Start with the parent tracking issue
- Create high-priority mapper issues first
- Add others as needed

### 3. Link Issues
- Update `#TBD` placeholders in PARENT_ISSUE.md with actual issue numbers
- Add appropriate labels (e.g., `enhancement`, `mapper`, priority labels)
- Link sub-issues to parent issue

### 4. Implementation Order (Recommended)

**Phase 1:** Quick wins with existing code
- Mapper 206 (based on MMC3)
- Mapper 10 (based on MMC2)
- Mapper 71 (based on UxROM)

**Phase 2:** Important standalone mappers
- Mapper 69 (Sunsoft FME-7)
- Mappers 21-25 (based on VRC6)

**Phase 3:** Complex but important
- Mapper 19 (Namco 163)

**Phase 4:** Completeness
- Mappers 13, 15, 34, 78

## Resources Consulted

During this analysis, the following authoritative sources were used:

- **NESdev Wiki** - Primary technical documentation
  - https://www.nesdev.org/wiki/List_of_mappers
  - Individual mapper pages with register specifications

- **NES Cart Database** - Game-to-mapper mappings
  - https://nescartdb.com/

- **Usage Statistics** - Game count data from community databases

- **Reference Implementations** - Open source emulators
  - FCEUX
  - Mesen
  - Nestopia

## Technical Notes

### Leveraging Existing Code

Several missing mappers can reuse existing neser implementations:

- **Mapper 206** → Similar to MMC3 (mapper 4)
- **Mapper 10** → Similar to MMC2 (mapper 9)
- **Mappers 21-25** → Similar to VRC6 (mappers 24/26)
- **Mapper 71** → Similar to UxROM (mapper 2)

This significantly reduces implementation complexity.

### Testing Strategy

Each mapper should be tested with:
1. At least one commercial game ROM
2. Test ROMs from NESdev (if available)
3. Verification that existing mapper tests still pass

### Code Quality

All implementations should follow neser's existing patterns:
- Implement the `Mapper` trait
- Create dedicated file in `src/cartridge/`
- Add case to `create_mapper()` function
- Include appropriate comments for unusual behavior
- Follow Rust best practices

## Conclusion

This analysis provides a complete roadmap for implementing missing NES mappers in neser. The provided templates contain all necessary information to create actionable GitHub issues that will guide the implementation process.

**Impact:** Implementing these mappers would increase game compatibility from ~85% to ~95%+, enabling popular titles like:
- Fire Emblem series
- Gimmick!
- Various Konami Famicom exclusives
- Namco titles
- And many more

The templates are production-ready and can be converted to GitHub issues immediately.
