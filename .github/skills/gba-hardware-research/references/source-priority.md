# Game Boy Advance Hardware Research Source Priority

The three tiers below are shared by every hardware research skill in this repository: one
specification authority, one or two implementation references, one screenshot reference.

## Tier 1: specification authority (leading)

1. **GBATek (problemkaputt)**
   - Primary source: `https://problemkaputt.de/gbatek.htm`
   - Start here for the memory map, CPU/PPU/APU/DMA/timer registers, cartridge and save
     hardware, BIOS functions, and timing notes.
   - Whole-page extraction works best: download once with
     `curl -Lsf https://problemkaputt.de/gbatek.htm -o <scratch>/gbatek.htm`, list anchors with
     `grep -oiE 'NAME="[^"]+"'`, and slice the section you need between consecutive anchors.

2. **Direct retrieval with `curl` or a mirror** (transport fallback, same source)
   - `curl -Lsf` the page; if problemkaputt.de is down, the NESdev wiki hosts a copy at
     `https://www.nesdev.org/wiki/Gameboy_Advance`.

3. **Supplements** (explain GBATek, never override it)
   - TONC: `https://www.coranac.com/tonc/` for graphics theory, affine math and worked code.
   - GBA Technical Reference for a second reading of address ranges and register layouts.
   - ARM7TDMI Technical Reference Manual for base instruction cycle counts.

4. **Hardware-verified test ROMs** (hardware observations, supplement GBATek)
   - mGBA test suite, AGS aging cartridge, jsmolka's gba-tests, armwrestler, under `roms/`.
   - A test ROM's assertion is a hardware observation. When it contradicts a GBATek
     sentence, report the test ROM as authoritative for that observable and say GBATek
     disagrees.

## Tier 2: implementation references (when the specification is not enough)

5. **mGBA** (first)
   - Repo: `https://github.com/mgba-emu/mgba` (check `../mgba` first)
   - Useful entry points:
     - `src/arm/` for ARM7TDMI instruction decoding and cycle counting
     - `src/gba/video.c` and `src/gba/renderers/` for PPU behavior
     - `src/gba/dma.c` for DMA scheduling and priority
     - `src/gba/timer.c` for timers
     - `src/gba/audio.c` for the APU and sound FIFOs
     - `src/gba/memory.c` for the memory map, wait states and open bus
     - `src/gba/savedata.c` and `src/gba/cart/` for save hardware and cartridge devices
     - `src/gba/io.c` for I/O register read/write masks
   - NESER can replay an mGBA trace log (`neser --gba-trace-mgba-log`, see README-GBA.md)
     to compare CPU state instruction by instruction.

6. **NanoBoyAdvance** (second, independent lineage)
   - Repo: `https://github.com/nba-emu/NanoBoyAdvance` (check `../NanoBoyAdvance` first)
   - Start in `src/nba/src/` (`arm/`, `hw/ppu/`, `hw/dma/`, `hw/apu/`, `bus/`).
   - Consult only when mGBA has no model for the behavior, or when a second independent
     implementation agreeing with mGBA would settle a question the specification leaves
     open. Distinguish counter-evidence (NanoBoyAdvance computes the same quantity
     differently) from absence of evidence (it is structured differently and has no model).

## Tier 3: screenshot reference

7. **mGBA**
   - The only emulator whose captures approve a NESER golden frame.
   - `~/repos/mgba/build/mgba-headless`, built from a clone with `-DBUILD_HEADLESS=ON
     -DENABLE_SCRIPTING=ON` plus `scripts/reference_capture/mgba-headless-video-buffer.patch`
     (stock headless has no framebuffer and writes empty PNGs). Verified recipe:
     `scripts/reference_capture/README.md`. In short: `CAPTURE_FRAME=<n> CAPTURE_OUT=<abs.png>
     mgba-headless --script scripts/reference_capture/mgba_capture.lua <rom.gba>`. HLE BIOS
     on both sides unless the ROM needs the real one.
   - Diff with `python -m scripts.diff_screenshots <neser> <mgba> --shift-search 1`.

## Reporting rules

- Prefer written specification over emulator implementation.
- If only mGBA or NanoBoyAdvance provides an answer, say that the statement is implementation-backed rather than specification-backed.
- If you still cannot confirm the behavior, state that the detail is unknown instead of inferring it.
