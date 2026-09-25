---
name: gba-hardware-research
description: Research Game Boy Advance hardware details. Specification authority is GBATek (with curl and mirror fallbacks, TONC and the GBA Technical Reference as supplements); mGBA is the implementation reference when the specification is incomplete, NanoBoyAdvance the second, and mGBA the screenshot reference for visual test ROMs.
---

# Game Boy Advance Hardware Research

## Introduction

Use this skill whenever you need details about any part of Game Boy Advance hardware. This includes CPU (ARM7TDMI), PPU, APU, DMA controller, memory map, cartridge types, save formats, timing, and model revisions. Prefer source-backed answers, be thorough, and never guess when documentation is missing or incomplete.

## Authorities

Every hardware research skill in this repository uses the same three tiers. The tiers are ranked: a lower tier is consulted only when the tier above it does not answer the question, and an answer is always labelled with the tier it came from.

1. **Specification authority: GBATek** (`https://problemkaputt.de/gbatek.htm`).
   The single leading source. It decides what the hardware does. Retrieval fallbacks (`curl`, mirrors) are transports for the same source, not different authorities. TONC and the GBA Technical Reference are supplements that explain GBATek's terse sections; they never override it. Hardware-verified test ROMs (the mGBA suite, AGS aging cartridge, jsmolka's gba-tests, armwrestler) are hardware observations: when one contradicts a GBATek sentence, report the test ROM as authoritative for that observable and say GBATek disagrees.

2. **Implementation references: mGBA, then NanoBoyAdvance.**
   Used only when the specification is missing, incomplete, or ambiguous, to see how a specific behavior can be implemented. mGBA (`https://github.com/mgba-emu/mgba`, `src/gba/` and `src/arm/`) is consulted first; NanoBoyAdvance (`https://github.com/nba-emu/NanoBoyAdvance`, `src/nba/src/`) is the second, independent, cycle-accurate lineage, consulted when mGBA has no model for the behavior or when two implementations agreeing would settle a question. Implementation evidence is never equal authority with the specification. Where an emulator makes a choice the specification does not settle, say so instead of presenting it as hardware fact.

3. **Screenshot reference: mGBA.**
   When a visual test ROM needs a reference image, capture it with mGBA at the same frame as NESER and pixel-diff the two captures with `python -m scripts.diff_screenshots`. An exact match approves the golden. If the captures differ and mGBA itself is suspect, ask the navigator instead of approving either side. The verified recipe uses mGBA's own `mgba-headless` tool built from source with Lua scripting and the framebuffer patch in `scripts/reference_capture/`, driven by `mgba_capture.lua`; it produced a 0-px match against NESER on the mGBA suite at frame 300 on 2026-09-25. `/Applications/mGBA.app` is the GUI and takes no script on its command line.

## Instructions

1. Define the target precisely before researching.

- Identify the hardware area, the exact behavior in question, and any revision or mode constraints.
- Distinguish between questions about specification, observed behavior, emulator behavior, and cartridge-specific wiring.

2. Start with GBATek as the specification authority.

- GBATek is the most comprehensive single-source GBA reference.
- Follow linked sections when topics span multiple components (CPU/DMA interactions, PPU-timer coordination, etc.).

3. Use this retrieval order when accessing documentation.

- First, try fetching GBATek directly; if that fails, fetch it with `curl -Lsf`, or use a mirror (the NESdev wiki hosts one at `https://www.nesdev.org/wiki/Gameboy_Advance`).
- Read TONC (`https://www.coranac.com/tonc/`) or the GBA Technical Reference to understand a GBATek section, never to replace it.
- Use mGBA and NanoBoyAdvance source code only when the specification is incomplete.

4. When researching CPU timing and cycle counts, cross-reference known traces.

- ARM7TDMI cycles vary: base cycle counts + memory access penalties (S/N model).
- For instruction accuracy, compare against the GBATek CPU timing tables, the ARM7TDMI reference manual, mGBA's `src/arm/` implementation, and known working test ROMs.
- Document S (sequential) vs. N (non-sequential) cycle classifications.

5. When researching PPU modes and rendering, account for affine transform complexity.

- Tile modes (0-2) support optional affine transforms (rotation/scaling).
- Bitmap modes (3-5) have fixed or partial affine support.
- Per-scanline affine matrix changes require H-blank coordination.
- Reference the TONC Affine Matrix section for fixed-point math details.

6. When researching DMA channels, verify priority arbitration.

- 4 DMA channels with priority (0 > 1 > 2 > 3, channel 0 highest; channels 1 and 2 are the sound FIFO channels).
- Priority stalling: a lower-priority DMA can be interrupted mid-transfer.
- Reference the GBATek DMA section and cross-check the mGBA implementation.

7. When researching save types, use ROM database heuristics.

- SRAM: typically 32KB-64KB battery-backed.
- EEPROM: I2C-like protocol, 512 bytes or 8KB.
- Flash: 64KB or 128KB with sector erasing, several manufacturer IDs.
- Detection often requires heuristics (memory access patterns, ROM header codes, or database lookups).

8. If specification coverage is missing or incomplete, inspect mGBA, then NanoBoyAdvance.

- Check for clones alongside the current repo first (`../mgba`, `../NanoBoyAdvance`); otherwise fetch from GitHub.
- Prefer `mgba-emu/mgba` and focus on `src/gba/` and `src/arm/`; entry points are listed in `references/source-priority.md`.
- Consult NanoBoyAdvance only when mGBA has no model for the behavior or a second, independent implementation is needed to settle a question. When both agree on unspecified behavior, note that explicitly; when they disagree, state both approaches and go back to GBATek and the test ROMs.
- Treat both as implementation evidence, not as equal authority with written specs.
- If an emulator makes choices where the specification is unclear, state that explicitly.

9. For visual verification, cross-check against mGBA as the screenshot reference.

- Capture with `CAPTURE_FRAME=<n> CAPTURE_OUT=<abs.png> mgba-headless --script scripts/reference_capture/mgba_capture.lua <rom.gba>` as `scripts/reference_capture/README.md` describes and pixel-diff programmatically with `python -m scripts.diff_screenshots <neser.png> <mgba.png> --shift-search 1`; exact matches become the golden. Both sides count frames from power-on, but when NESER runs its built-in BIOS (mGBA without `-b`), pass `--skip-bios-intro`: that BIOS's intro otherwise delays the cartridge by about 255 frames, while mGBA's HLE BIOS has none. mGBA's no-BIOS boot also starts the cartridge at VCOUNT 126, so a ROM still drawing its first screen can be one frame ahead in mGBA; compare at a frame where the screen is static.
- Pin everything that can vary between runs on both sides (BIOS, power-on RAM state, frame skipping) and capture the reference twice before trusting any non-zero diff.
- If NESER and mGBA disagree and the divergence is suspected to be an mGBA quirk, ask the navigator how to proceed rather than approving either side unilaterally.
- Document the approval (frame, diff result) in the test comment.

10. When sources disagree or remain ambiguous, report that directly.

- Name the conflicting sources.
- State which source is more authoritative for the question at hand and why.
- Do not merge conflicting claims into a guessed answer.

11. Produce a detailed, source-backed answer.

- Start with a high-level explanation of the hardware behavior.
- Then cover precise details: registers, bit meanings, address ranges, timing, ordering, side effects, open-bus behavior, edge cases, and revision differences.
- Clearly label what is confirmed by specification, what is supported only by emulator implementation, and what is still unknown.
- Cite the exact GBATek sections, supplementary pages, or mGBA/NanoBoyAdvance files you consulted.

12. Never guess.

- If no authoritative information is available, say so plainly.
- If available information is partial, answer only the supported part and identify the gaps.

## References

- `references/source-priority.md`: source order, retrieval tips, and mGBA/NanoBoyAdvance lookup starting points.

## Examples

- Researching memory map layout:
  start with the GBATek memory section, then the GBA Technical Reference for a second explanation of address ranges and I/O register layout.

- Researching PPU tile mode rendering:
  start with the GBATek PPU section, TONC Graphics pages for theory, then mGBA `src/gba/video.c` if edge cases remain unclear.

- Researching DMA priority arbitration:
  start with the GBATek DMA section for channel priority, then verify against mGBA `src/gba/dma.c`.

- Researching ARM7TDMI instruction timing:
  start with the GBATek CPU section and the ARM7TDMI reference manual for base cycle counts, then cross-check mGBA `src/arm/` for GBA-specific penalties.

- Approving a golden frame for a visual test ROM:
  capture the same frame in mGBA and NESER with the same BIOS (NESER's built-in one with `--skip-bios-intro`), diff them with `scripts.diff_screenshots`, and record the frame and the 0-px result in the test comment.

## Known Hardware Gotchas

When writing code that targets GBA hardware (assembly or emulation), always verify against these known pitfalls:

- **VRAM byte writes**: GBA VRAM does not support byte writes (STRB). A byte write to VRAM duplicates the byte into both bytes of the addressed halfword. Use LDRH/STRH read-modify-write for individual pixel placement in bitmap modes. OBJ VRAM ignores byte writes entirely.
- **ARM alignment requirements**: LDRH/STRH require halfword-aligned addresses. Data structures accessed via LDRH must maintain 2-byte alignment throughout (e.g., RLE data using mixed .hword/.byte entries will misalign after the first entry).
- **HALTCNT register**: Writing to 0x04000301 halts the CPU until an enabled interrupt fires. The proprietary BIOS depends on this for VBlank timing during boot.
