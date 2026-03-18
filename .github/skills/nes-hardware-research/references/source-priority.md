# NES Hardware Research Source Priority

Use this order when researching NES or Famicom hardware details:

1. **NESdev current wiki**
   - Primary source: `https://www.nesdev.org/wiki/<Page>`
   - Start here for specification details, terminology, register behavior, timing notes, and mapper documentation.

2. **Direct retrieval with `curl`**
   - If the normal page fetch fails, try retrieving the same NESdev page directly with `curl -Lsf`.
   - Use this only as a transport fallback, not as a different source.

3. **NESdev mirror on nes.science**
   - Index: `https://nesdev-wiki.nes.science/wikipages/Special_AllPages.xhtml`
   - Pages are mirrored as `.xhtml`, for example `APU.xhtml` or `PPU_registers.xhtml`.
   - Use the All Pages index when the page title is uncertain.

4. **Mesen implementation for missing details**
   - Primary repo: `https://github.com/SourMesen/Mesen2`
   - Start in `Core/NES/`
   - Useful entry points:
     - `Core/NES/APU/` for APU behavior
     - `Core/NES/Input/` for controllers and peripherals
     - `Core/NES/Mappers/` for mapper logic
     - `Core/NES/NesCpu.cpp` for CPU behavior
     - `Core/NES/BaseNesPpu.cpp` and related PPU files for PPU behavior
     - `Core/NES/NesMemoryManager.cpp` and `Core/NES/NesConsole.cpp` for memory-map and system-level interactions
     - `Core/NES/BaseMapper.cpp` and `Core/NES/MapperFactory.cpp` for cartridge handling

5. **Legacy Mesen repo only if necessary**
   - Secondary repo: `https://github.com/SourMesen/Mesen`
   - Use this only when Mesen2 does not contain the relevant subsystem detail or when historical implementation behavior matters.

## Reporting rules

- Prefer written specification over emulator implementation.
- If only Mesen provides an answer, say that the statement is implementation-backed rather than specification-backed.
- If you still cannot confirm the behavior, state that the detail is unknown instead of inferring it.
