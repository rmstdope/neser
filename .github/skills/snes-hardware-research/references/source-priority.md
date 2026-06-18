# SNES Hardware Research Source Priority

Use this order when researching SNES / Super Famicom hardware details:

1. **fullsnes (problemkaputt)**
   - Primary source: `https://problemkaputt.de/fullsnes.htm`
   - Start here for the memory map, CPU/PPU/APU registers, DMA/HDMA, timing notes, cartridge mapping, and enhancement-chip documentation.
   - Most comprehensive single-source SNES reference; treat as the primary authority.
   - **Anchor/pagination tip:** fetching a section anchor (e.g. `#snespictureprocessingunitppu`, `#snespputimersandstatus`) usually returns the top-level I/O map first, not the detailed register text. To reach the deep descriptions (register bit layouts, formulas, access timing), fetch the PPU/APU page with increasing `start_index` values — for the PPU section the detailed text lives roughly around offsets 16000, 28000, 40000, 56000, 72000, 82000. Page forward until you see the specific register name you need.

2. **Direct retrieval with `curl`**
   - If the normal page fetch fails, retrieve fullsnes (or a mirror) directly with `curl -Lsf`.
   - Use this only as a transport fallback, not as a different source.

3. **anomie's SNES documents + SNESdev wiki**
   - anomie's docs: CPU (`anomie's 65816 doc`), PPU (regs/timing parts 1–2), DSP/APU, SPC700, memory map, timing.
   - SNESdev / Super Famicom wiki: `https://snes.nesdev.org/wiki/` and `https://wiki.superfamicom.org/`.
   - Use these for register bit meanings, timing diagrams, and behavior fullsnes summarizes only briefly.

4. **Test-ROM references for precise behavior**
   - 65816 CPU: Tom Harte / ProcessorTests `65816` vectors.
   - APU/SPC700 + S-DSP: blargg's SNES APU and SPC700 tests.
   - PPU: PeterLemon / krom SNES PPU test ROMs.
   - Treat passing/failing test vectors as authoritative for observable behavior.

5. **bsnes / higan implementation for missing details**
   - Primary repo: `https://github.com/bsnes-emu/bsnes` (or higan).
   - Useful entry points:
     - `bsnes/sfc/cpu/` and `processor/wdc65816/` for 65816 behavior
     - `bsnes/sfc/ppu/` for PPU rendering and timing
     - `bsnes/sfc/smp/` (SPC700) and `bsnes/sfc/dsp/` for audio
     - `bsnes/sfc/cartridge/` and `bsnes/sfc/memory/` for mapping and save hardware
     - `bsnes/sfc/controller/` for input devices
   - Use bsnes/higan only after checking fullsnes, anomie's docs, and the SNESdev wiki.
   - Treat bsnes/higan as implementation evidence, not as equal authority with written specifications.

## Reporting rules

- Prefer written specification (fullsnes, anomie) over emulator implementation.
- If only bsnes/higan provides an answer, say the statement is implementation-backed rather than specification-backed.
- If you still cannot confirm the behavior, state that the detail is unknown instead of inferring it.
