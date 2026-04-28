---
name: gba-hardware-research
description: Research Game Boy Advance hardware details from GBATek first, with GBA Technical Reference, TONC, and mGBA implementation as fallbacks.
---

# Game Boy Advance Hardware Research

## Introduction

Use this skill whenever you need details about any part of Game Boy Advance hardware. This includes CPU (ARM7TDMI), PPU, APU, DMA controller, memory map, cartridge types, save formats, timing, and model revisions. Prefer source-backed answers, be thorough, and never guess when documentation is missing or incomplete.

## Instructions

1. Define the target precisely before researching.

- Identify the hardware area, the exact behavior in question, and any revision or mode constraints.
- Distinguish between questions about specification, observed behavior, emulator behavior, and cartridge-specific wiring.

2. Start with GBATek as the primary source.

- GBATek is the most comprehensive single-source GBA reference (https://problemkaputt.de/gbatek.htm or mirror)
- Follow linked sections when topics span multiple components (CPU/DMA interactions, PPU-timer coordination, etc.)
- Treat GBATek as the primary authority for hardware specification details.

3. Use this retrieval order when accessing documentation.

- First, try fetching GBATek directly.
- If GBATek is unavailable, try the NESdev wiki GBA pages (https://www.nesdev.org/wiki/Gameboy_Advance).
- If still unavailable, try the mirrored GBA Tech Reference or TONC (https://www.coranac.com/tonc/).
- Use mGBA source code (https://github.com/mgba-emu/mgba) only when specs are incomplete.

4. When researching CPU timing and cycle counts, cross-reference known traces.

- ARM7TDMI cycles vary: Base cycle counts + memory access penalties (S/N model)
- For instruction accuracy, compare against:
  - NO$GBA instruction timing tables (if available)
  - mGBA CPU implementation (`src/arm/core.c`)
  - Known working test ROMs and their behavior
- Document S (sequential) vs. N (non-sequential) cycle classifications

5. When researching PPU modes and rendering, account for affine transforms complexity.

- Tile modes (0-2) support optional affine transforms (rotation/scaling)
- Bitmap modes (3-5) have fixed or partial affine support
- Per-scanline affine matrix changes require H-blank coordination
- Reference TONC Affine Matrix section for fixed-point math details

6. When researching DMA channels, verify priority arbitration.

- 4 DMA channels with priority (3 > 2 > 1 > 0)
- Channel 3 highest priority (used for APU audio)
- Priority stalling: lower-priority DMA can be interrupted mid-transfer
- Reference GBATek DMA section and cross-check mGBA implementation

7. When researching save types, use ROM database heuristics.

- SRAM: typically 32KB-64KB battery-backed
- EEPROM: I2C-like protocol, 512 bytes or 8KB
- Flash: Panasonic MN63F805MNP, 64KB or 128KB with sector erasing
- Detection often requires heuristics (memory access patterns, ROM header codes, or database lookups)

8. If specification coverage is missing or incomplete, inspect mGBA carefully.

- Prefer `mgba-emu/mgba` and focus on `src/` for hardware implementation details
- Use mGBA source only after checking GBATek, GBA Tech Ref, and TONC
- Treat mGBA as implementation evidence, not as equal authority with written specs
- If mGBA makes choices where the specification is unclear, state that explicitly

9. When sources disagree or remain ambiguous, report that directly.

- Name the conflicting sources
- State which source is more authoritative for the question at hand and why
- Do not merge conflicting claims into a guessed answer

10. Produce a detailed, source-backed answer.

- Start with a high-level explanation of the hardware behavior
- Then cover precise details: registers, bit meanings, address ranges, timing, ordering, side effects, open-bus behavior, edge cases, and revision differences
- Clearly label what is confirmed by specification, what is supported only by emulator implementation, and what is still unknown
- Cite the exact GBATek sections, GBA Technical Reference pages, or mGBA files you consulted

11. Never guess.

- If no authoritative information is available, say so plainly
- If available information is partial, answer only the supported part and identify the gaps

## References

- `references/source-priority.md`: source order, retrieval tips, and mGBA lookup starting points

## Examples

- Researching memory map layout:
  start with GBATek memory section, then GBA Tech Ref for detailed address ranges and I/O register layout

- Researching PPU tile mode rendering:
  start with GBATek PPU section, TONC Graphics pages for theory, then mGBA `src/gba/video.c` if edge cases remain unclear

- Researching DMA priority arbitration:
  start with GBATek DMA section for channel priority, then verify against mGBA `src/gba/dma.c` implementation

- Researching ARM7TDMI instruction timing:
  start with ARM Architecture Reference for basic cycle counts, then cross-check GBATek CPU section and mGBA `src/arm/core.c` for GBA-specific penalties
