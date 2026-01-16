# Missing Mappers Implementation Tracking

## Overview
This is a tracking issue for implementing missing NES mapper support in neser. Currently, neser supports 13 mapper types covering approximately 85-88% of all NES/Famicom games. This issue tracks the implementation of additional mappers to increase game compatibility.

## Currently Implemented Mappers
- ✅ Mapper 0 (NROM)
- ✅ Mapper 1 (MMC1/SxROM)
- ✅ Mapper 2 (UxROM)
- ✅ Mapper 3 (CNROM)
- ✅ Mapper 4 (MMC3/TxROM)
- ✅ Mapper 5 (MMC5/ExROM)
- ✅ Mapper 7 (AxROM)
- ✅ Mapper 9 (MMC2/PxROM)
- ✅ Mapper 11 (Color Dreams)
- ✅ Mapper 16 (Bandai FCG)
- ✅ Mapper 24 (VRC6a)
- ✅ Mapper 26 (VRC6b)
- ✅ Mapper 66 (GxROM/MxROM)

## Missing Mappers (by Priority)

### High Priority
- [ ] #TBD - Mapper 206 (Namco 118/DxROM) - ~44 games
- [ ] #TBD - Mapper 19 (Namco 163) - ~20 games
- [ ] #TBD - Mapper 10 (MMC4/FxROM) - 3 key Famicom titles
- [ ] #TBD - Mapper 69 (Sunsoft FME-7) - Notable games

### Medium Priority (VRC variants)
- [ ] #TBD - Mapper 21 (VRC4a/VRC4c)
- [ ] #TBD - Mapper 22 (VRC2a)
- [ ] #TBD - Mapper 23 (VRC2b/VRC4e)
- [ ] #TBD - Mapper 25 (VRC4b/VRC4d)

### Lower Priority
- [ ] #TBD - Mapper 71 (Camerica/Codemasters)
- [ ] #TBD - Mapper 34 (BNROM/NINA-001)
- [ ] #TBD - Mapper 13 (CPROM)
- [ ] #TBD - Mapper 78 (NINA-03/NINA-06)
- [ ] #TBD - Mapper 15 (100-in-1 multicart)

## Impact
Implementing these mappers would:
- Increase game compatibility from ~85% to 95%+ of all NES/Famicom games
- Enable popular titles like Gimmick!, Fire Emblem series, and various Konami Famicom exclusives
- Improve support for Namco titles

## Resources
- [NESdev Wiki - List of Mappers](https://www.nesdev.org/wiki/List_of_mappers)
- [NES Cart Database](https://nescartdb.com/)

Each sub-issue contains specific implementation details, documentation links, and known games for that mapper.
