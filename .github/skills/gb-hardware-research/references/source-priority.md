# Game Boy Hardware Research Source Priority

The three tiers below are shared by every hardware research skill in this repository: one
specification authority, one or two implementation references, one screenshot reference.

## Tier 1: specification authority (leading)

1. **Pan Docs (gbdev.io)**
   - Primary source: `https://gbdev.io/pandocs/<page>`
   - Start here for specification details, terminology, register behavior, timing notes, and MBC documentation.
   - Key pages include: `CPU`, `PPU`, `APU`, `Memory_Map`, `Joypad_Input`, `Serial_Data_Transfer`, `Timer_and_Divider_Registers`, `Interrupts`, `OAM_DMA_Transfer`, `MBCs`, `CGB_Registers`, `SGB_Functions`.

2. **Direct retrieval with `curl`** (transport fallback, same source)
   - If the normal page fetch fails, try retrieving the same Pan Docs page directly with `curl -Lsf`.

3. **Pan Docs GitHub source** (transport fallback, same source)
   - Repository: `https://github.com/gbdev/pandocs`
   - Markdown source files are in `src/`, for example `src/CPU_Instruction_Set.md` or `src/Rendering.md`.

4. **Hardware-verified test ROMs** (hardware observations, supplement Pan Docs)
   - Mooneye test suite sources: `https://raw.githubusercontent.com/Gekkio/mooneye-test-suite/main/<path>.s`
   - blargg's cpu_instrs, instr_timing, mem_timing and dmg_sound/cgb_sound suites.
   - A test ROM's assertion is a hardware observation. When it contradicts a Pan Docs
     sentence, report the test ROM as authoritative for that observable and say Pan Docs
     disagrees.

## Tier 2: implementation references (when the specification is not enough)

5. **SameBoy** (first)
   - Repo: `https://github.com/LIJI32/SameBoy` (check `../SameBoy` first)
   - Start in `Core/`
   - Useful entry points:
     - `Core/apu.c` and `Core/apu.h` for APU behavior
     - `Core/display.c` and `Core/display.h` for PPU/display behavior
     - `Core/sm83_cpu.c` for CPU behavior and instruction timing
     - `Core/memory.c` for memory map, I/O registers, and bus behavior
     - `Core/joypad.c` for joypad handling
     - `Core/timing.c` for system timing and clock dividers
     - `Core/mbc.c` for memory bank controller implementations
     - `Core/serial.c` for serial port behavior
     - `Core/gb.c` and `Core/gb.h` for system-level orchestration and model differences
     - `Core/sgb.c` for Super Game Boy specific behavior
     - `Core/camera.c` for Game Boy Camera MBC

6. **Gambatte** (second, independent lineage)
   - Repo: `https://github.com/sinamas/gambatte` (check `../gambatte` first)
   - Start in `libgambatte/src/`.
   - Consult only when SameBoy has no model for the behavior, or when a second independent
     implementation agreeing with SameBoy would settle a question the specification leaves
     open. Distinguish counter-evidence (Gambatte computes the same quantity differently)
     from absence of evidence (Gambatte is structured differently and has no model for it).

## Tier 3: screenshot reference

7. **SameBoy**
   - The only emulator whose captures approve a NESER golden frame.
   - `~/repos/SameBoy/build/bin/tester/sameboy_tester` (`make tester` in a SameBoy clone)
     with the boot ROMs from `/Applications/SameBoy.app/Contents/Resources/`. Verified
     recipe and its time-based caveat: `scripts/reference_capture/README.md`. In short:
     `sameboy_tester --dmg --length 10 --boot .../dmg_boot.bin <rom.gb>` writes `<rom>.bmp`
     next to the ROM; convert with Pillow before diffing.
   - Diff with `python -m scripts.diff_screenshots <neser> <sameboy> --shift-search 1`.

## Reporting rules

- Prefer written specification over emulator implementation.
- If only SameBoy or Gambatte provides an answer, say that the statement is implementation-backed rather than specification-backed.
- If you still cannot confirm the behavior, state that the detail is unknown instead of inferring it.
