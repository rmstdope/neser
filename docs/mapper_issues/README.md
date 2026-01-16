# NES Mapper Implementation Issue Templates

This directory contains GitHub issue templates for implementing missing NES mappers in the neser emulator.

## Structure

- **PARENT_ISSUE.md** - Master tracking issue for all missing mappers
- **mapper_206.md** - Mapper 206 (Namco 118) - HIGH priority
- **mapper_19.md** - Mapper 19 (Namco 163) - HIGH priority  
- **mapper_10.md** - Mapper 10 (MMC4) - MEDIUM-HIGH priority
- **mapper_69.md** - Mapper 69 (Sunsoft FME-7) - MEDIUM-HIGH priority
- **mappers_21_25.md** - Mappers 21-25 (VRC2/VRC4) - MEDIUM priority
- **mapper_71.md** - Mapper 71 (Camerica) - LOW-MEDIUM priority
- **remaining_low_priority.md** - Mappers 13, 15, 34, 78 - LOW priority

## Usage

These templates should be used to create GitHub issues in the neser repository. Each template includes:

- Overview and priority
- Hardware specifications
- Implementation details and strategy
- Documentation resources with links
- List of known games using the mapper
- Technical considerations
- Acceptance criteria

## Creating Issues

**Note:** The agent cannot create GitHub issues directly. The user or repository maintainer should:

1. Review each template file
2. Create a new GitHub issue for each template
3. Copy the content from the template into the issue body
4. Update the `#TBD` placeholders in PARENT_ISSUE.md with actual issue numbers
5. Apply appropriate labels (e.g., `enhancement`, `mapper`, priority labels)
6. Link sub-issues to the parent tracking issue

## Priority Order

Recommended implementation order based on impact:

1. **High Priority (Top 10 mappers):**
   - Mapper 206 (Namco 118) - ~44 games, simple implementation
   - Mapper 19 (Namco 163) - ~20 games, complex but important

2. **Medium-High Priority (Notable games):**
   - Mapper 10 (MMC4) - 3 key games (Fire Emblem, Famicom Wars)
   - Mapper 69 (Sunsoft FME-7) - Gimmick!, Batman: Return of the Joker

3. **Medium Priority (Konami games):**
   - Mappers 21-25 (VRC2/VRC4) - Various Konami Famicom games

4. **Lower Priority:**
   - Mapper 71 (Camerica) - Codemasters games
   - Mappers 13, 15, 34, 78 - Limited game counts

## Impact

Current implementation: 13 mapper types (~85-88% game coverage)
After high-priority mappers: ~90% game coverage
After all proposed mappers: ~95%+ game coverage

## Resources

All templates include links to:
- NESdev Wiki documentation
- Reference implementations in neser codebase
- Game databases
- Community resources
