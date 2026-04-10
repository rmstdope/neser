# Game Boy Hardware Research Source Priority

Use this order when researching Game Boy or Game Boy Color hardware details:

1. **Pan Docs (gbdev.io)**
   - Primary source: `https://gbdev.io/pandocs/<page>`
   - Start here for specification details, terminology, register behavior, timing notes, and MBC documentation.
   - Key pages include: `CPU`, `PPU`, `APU`, `Memory_Map`, `Joypad_Input`, `Serial_Data_Transfer`, `Timer_and_Divider_Registers`, `Interrupts`, `OAM_DMA_Transfer`, `MBCs`, `CGB_Registers`, `SGB_Functions`.

2. **Direct retrieval with `curl`**
   - If the normal page fetch fails, try retrieving the same Pan Docs page directly with `curl -Lsf`.
   - Use this only as a transport fallback, not as a different source.

3. **Pan Docs GitHub source**
   - Repository: `https://github.com/gbdev/pandocs`
   - Markdown source files are in `src/`, for example `src/CPU_Instruction_Set.md` or `src/Rendering.md`.
   - Use the repository when the hosted version is unavailable or when you need the raw content.

4. **SameBoy implementation for missing details**
   - Primary repo: `https://github.com/LIJI32/SameBoy`
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

5. **Gambatte as secondary emulator reference**
   - Repository: `https://github.com/sinamas/gambatte`
   - Use this only when SameBoy does not contain the relevant subsystem detail or when cross-referencing implementation behavior against a second high-accuracy emulator matters.
   - Start in `libgambatte/src/`.

## Reporting rules

- Prefer written specification over emulator implementation.
- If only SameBoy provides an answer, say that the statement is implementation-backed rather than specification-backed.
- If you still cannot confirm the behavior, state that the detail is unknown instead of inferring it.
