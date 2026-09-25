# NES Hardware Research Source Priority

The three tiers below are shared by every hardware research skill in this repository: one
specification authority, one or two implementation references, one screenshot reference.

## Tier 1: specification authority (leading)

1. **NESdev wiki**
   - Primary source: `https://www.nesdev.org/wiki/<Page>`
   - Start here for specification details, terminology, register behavior, timing notes, and mapper documentation.

2. **Direct retrieval with `curl`** (transport fallback, same source)
   - If the normal page fetch fails, try retrieving the same NESdev page directly with `curl -Lsf`.

3. **NESdev mirror on nes.science** (transport fallback, same source)
   - Index: `https://nesdev-wiki.nes.science/wikipages/Special_AllPages.xhtml`
   - Pages are mirrored as `.xhtml`, for example `APU.xhtml` or `PPU_registers.xhtml`.
   - Use the All Pages index when the page title is uncertain.

4. **Hardware-verified test ROMs** (hardware observations, supplement the wiki)
   - blargg's CPU/PPU/APU suites and the nes-test-roms collection under `roms/`.
   - A test ROM's assertion is a hardware observation. When it contradicts a wiki sentence,
     report the test ROM as authoritative for that observable and say the wiki disagrees.

## Tier 2: implementation reference (when the specification is not enough)

5. **Mesen2**
   - Repo: `https://github.com/SourMesen/Mesen2` (check `../Mesen2` first)
   - Start in `Core/NES/`
   - Useful entry points:
     - `Core/NES/APU/` for APU behavior
     - `Core/NES/Input/` for controllers and peripherals
     - `Core/NES/Mappers/` for mapper logic
     - `Core/NES/NesCpu.cpp` for CPU behavior
     - `Core/NES/BaseNesPpu.cpp` and related PPU files for PPU behavior
     - `Core/NES/NesMemoryManager.cpp` and `Core/NES/NesConsole.cpp` for memory-map and system-level interactions
     - `Core/NES/BaseMapper.cpp` and `Core/NES/MapperFactory.cpp` for cartridge handling
   - No second implementation reference is designated for the NES. The legacy
     `https://github.com/SourMesen/Mesen` repository is history only: consult it when a
     Mesen2 change needs its rationale, never as an authority.

## Tier 3: screenshot reference

6. **Mesen2**
   - The only emulator whose captures approve a NESER golden frame.
   - Binary: `/Applications/Mesen.app/Contents/MacOS/Mesen` (official release zip; no
     Homebrew cask). Verified recipe, scripts and the `AllowIoOsAccess` toggle:
     `scripts/reference_capture/README.md`. In short:
     `CAPTURE_FRAME=<n> CAPTURE_OUT=<abs.png> Mesen --testRunner --enableStdout --timeout=30
     --Video.VideoFilter=None --Video.AspectRatio=NoStretching --nes.DisableFrameSkipping=true
     --nes.RamPowerOnState=AllZeros <rom> scripts/reference_capture/mesen2_capture.lua`.
   - Diff with `python -m scripts.diff_screenshots <neser> <mesen> --shift-search 1`.

## Reporting rules

- Prefer written specification over emulator implementation.
- If only Mesen2 provides an answer, say that the statement is implementation-backed rather than specification-backed.
- If you still cannot confirm the behavior, state that the detail is unknown instead of inferring it.
