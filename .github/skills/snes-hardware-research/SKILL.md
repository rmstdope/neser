---
name: snes-hardware-research
description: Research Super Nintendo (SNES/Super Famicom) hardware details from fullsnes first, with anomie's docs and the SNESdev wiki as fallbacks, and bsnes/higan implementation only when specs are incomplete.
---

# SNES Hardware Research

## Introduction

Use this skill whenever you need details about any part of Super Nintendo Entertainment System (SNES) / Super Famicom hardware. This includes the 65816 CPU, PPU (1 and 2), the SPC700 audio CPU and S-DSP, the APU IPL boot ROM and 64 KB ARAM, DMA/HDMA, the memory map (LoROM/HiROM/ExHiROM), cartridge/board behavior, save hardware (battery SRAM), controller ports and peripherals, timing, electrical quirks, enhancement chips, and console/region (NTSC/PAL) differences. Prefer source-backed answers, be thorough, and never guess when documentation is missing or incomplete.

## Instructions

1. Define the target precisely before researching.
   - Identify the hardware area, the exact behavior in question, and any model, revision, or region (NTSC/PAL) constraints.
   - Distinguish between questions about specification, observed behavior, emulator behavior, and board/cartridge-specific wiring.

2. Start with fullsnes as the primary source.
   - fullsnes (problemkaputt) is the most comprehensive single-source SNES reference (`https://problemkaputt.de/fullsnes.htm` or a mirror).
   - Follow linked sections when topics span multiple components (CPU/PPU timing, DMA/HDMA-PPU coordination, APU port handshakes, etc.).
   - Treat fullsnes as the primary authority for hardware specification details.

3. Use this retrieval order when accessing documentation.
   - First, try fetching fullsnes directly.
   - If the page cannot be retrieved with standard tools, try fetching it directly with `curl -Lsf`.
   - If fullsnes is unavailable, use anomie's SNES documents (CPU, PPU, registers, timing, DSP) and the SNESdev wiki (`https://snes.nesdev.org/wiki/` / `https://wiki.superfamicom.org/`).
   - Use bsnes / higan source code (`https://github.com/bsnes-emu/bsnes`, higan) only when specs are incomplete.

4. When researching 65816 CPU timing and cycle counts, account for variable memory speed.
   - Memory access speed depends on region (FastROM vs SlowROM, MEMSEL `$420D`) and the accessed bank/address.
   - **Fetch the `#snesmemorycontrol` anchor of fullsnes first** — it contains the authoritative per-region speed table and MEMSEL register definition.
   - The three SNES bus speeds are: **Fast 3.58 MHz (6 master clocks)**, **Slow 2.68 MHz (8 master clocks)**, **XSlow 1.78 MHz (12 master clocks)**.
   - Key regions that are commonly mis-classified: B-Bus I/O `$2000–$3FFF` and CPU I/O `$4200–$5FFF` in banks `$00–$3F`/`$80–$BF` are **Fast (6 clocks)**, not slow. Only WRAM mirrors (`$0000–$1FFF`) and expansion (`$6000–$7FFF`) are Slow (8 clocks) in those banks.
   - WS1 ROM (banks `$00–$3F`:`$8000–$FFFF`, `$40–$7D`) is **always slow (8 clocks)**; MEMSEL only affects WS2 ROM (banks `$80–$BF`:`$8000–$FFFF` and `$C0–$FF`).
   - Document cycle penalties for MMIO, WRAM, and cartridge regions separately from base instruction cycles.
   - Cross-check against anomie's timing docs and Tom Harte / ProcessorTests 65816 vectors; treat bsnes as implementation evidence only.

5. When researching PPU modes and rendering, separate the many features.
   - BG modes 0–7 differ in layer count and bit depth; Mode 7 adds an affine matrix (rotation/scaling).
   - Account for windows, color math (add/subtract, half), mosaic, offset-per-tile (modes 2/4/6), hi-res/pseudo-hires (512-wide), interlace, and the OBJ (sprite) range/time over-limits.
   - Verify V/H counters, NMI, and the H/V IRQ (`$4207`–`$420A`) timing precisely.

6. When researching the APU, treat it as a separate self-contained system.
   - The SPC700 CPU + S-DSP share 64 KB ARAM; the main CPU communicates only through four I/O ports (`$2140`–`$2143` ⟷ `$F4`–`$F7`).
   - Cover the 64-byte IPL boot ROM handshake, SPC700 timers, S-DSP 8 voices, BRR block decoding, ADSR/GAIN envelopes, gaussian interpolation, echo buffer and 8-tap FIR filter.
   - Cross-check against blargg's SNES APU/SPC700 tests; treat bsnes/higan DSP source as implementation evidence only.

7. When researching the memory map and cartridges, verify mapping and save hardware.
   - LoROM, HiROM, ExHiROM mapping is auto-detected from the internal header (`$FFC0` area) plus heuristics; document the bank/offset math for each.
   - Battery SRAM size/presence comes from the header; document `.srm` layout expectations.
   - Note copier headers (512-byte) on `.smc` files and how to detect/strip them.

8. If specification coverage is missing or incomplete, inspect bsnes/higan carefully.
   - Prefer `bsnes-emu/bsnes` and focus on the `bsnes/sfc/` (or higan `sfc/`) cores.
   - Use this source only after checking fullsnes, anomie's docs, and the SNESdev wiki.
   - Treat bsnes/higan as implementation evidence, not as equal authority with written specifications.
   - If bsnes makes a choice where the specification is unclear, state that explicitly.

9. When sources disagree or remain ambiguous, report that directly.
   - Name the conflicting sources.
   - State which source is more authoritative for the question at hand and why.
   - Do not merge conflicting claims into a guessed answer.

10. Produce a detailed, source-backed answer.
    - Start with a high-level explanation of the hardware behavior.
    - Then cover precise details: registers, bit meanings, address ranges, timing, ordering, side effects, open-bus behavior, edge cases, and model/region differences.
    - Clearly label what is confirmed by specification, what is supported only by emulator implementation, and what is still unknown.
    - Cite the exact fullsnes sections, anomie doc names, SNESdev wiki pages, or bsnes/higan files you consulted.

11. Never guess.
    - If no authoritative information is available, say so plainly.
    - If available information is partial, answer only the supported part and identify the gaps.

## References

- `references/source-priority.md`: source order, retrieval tips, and bsnes/higan lookup starting points.

## Examples

- Researching the memory map / LoROM vs HiROM mapping:
  start with the fullsnes memory map section, cross-check anomie's memory-map doc, then bsnes `sfc/cartridge/` only if heuristics remain unclear.

- Researching Mode 7 rendering:
  start with the fullsnes PPU section and anomie's PPU doc for the affine matrix math, then inspect bsnes `sfc/ppu/` if per-scanline edge cases remain unclear.

- Researching the APU I/O port handshake:
  start with fullsnes APU section for the IPL boot protocol, cross-check anomie's APU/DSP docs, then verify against blargg's SPC700 tests and bsnes `sfc/smp/` / `sfc/dsp/`.

- Researching 65816 instruction timing:
  start with anomie's timing doc and fullsnes CPU section, then cross-check Tom Harte / ProcessorTests 65816 vectors and bsnes `processor/wdc65816/`.

## Known Hardware Gotchas

When writing code that targets or emulates SNES hardware, always verify against these known pitfalls:

- **65816 mode flags**: The M (accumulator) and X (index) width flags change register sizes at runtime; emulation mode (E) forces 8-bit and re-maps the stack to page 1. Misinterpreting flag state corrupts decode width and cycle counts.
- **Open bus / MDR**: Reads of unmapped or write-only addresses return the last value on the memory data register (open bus), not zero.
- **HDMA vs DMA timing**: HDMA steals cycles from the CPU per scanline and has subtle init/repeat/line-counter semantics; general-purpose DMA pauses the CPU entirely. Both interact with PPU access windows.
- **APU is asynchronous**: The SPC700 runs on its own ~1.024 MHz clock, independent of the main CPU. Port reads/writes are the only synchronization; many games rely on exact IPL upload timing.
- **VRAM access timing**: CPU access to VRAM/CGRAM/OAM is only safe during V/H-blank (or forced blank); the address-increment-on-read/write behavior of `$2116`–`$2119` is a frequent source of bugs.
- **Copier headers**: `.smc` files may carry a 512-byte header; failing to detect/strip it offsets the entire ROM and breaks mapping detection.
- **I/O region speeds are Fast, not Slow**: B-Bus I/O (`$2000–$3FFF`) and CPU I/O (`$4200–$5FFF`) in system banks run at 3.58 MHz (6 master clocks), the same as FastROM. Only WRAM mirrors (`$0000–$1FFF`) and expansion (`$6000–$7FFF`) in system banks are slow (8 clocks). This is a common planning mistake when building cycle-accurate bus models.
