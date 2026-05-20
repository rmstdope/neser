---
name: gb-hardware-research
description: Research Game Boy/Game Boy Color hardware details from Pan Docs first, with curl and mirror fallbacks, and SameBoy implementation only when specs are incomplete.
---

# Game Boy Hardware Research

## Introduction

Use this skill whenever you need details about any part of Game Boy or Game Boy Color hardware. This includes CPU (SM83/LR35902), PPU, APU, DMA, serial port, joypad, cartridge bus behavior, memory maps, timing, electrical quirks, MBC (memory bank controllers), and model differences (DMG, MGB, CGB, SGB). Prefer source-backed answers, be thorough, and never guess when the documentation is missing or incomplete.

## Instructions

1. Define the target precisely before researching.

- Identify the hardware area, the exact behavior in question, and any model or revision constraints.
- Distinguish between questions about specification, observed behavior, emulator behavior, and MBC-specific wiring.

2. Start with Pan Docs as the primary source.

- Look for the most specific Pan Docs page first.
- Read linked pages when the topic spans multiple components, such as CPU/PPU timing, joypad I/O, DMA interactions, or MBC-specific behavior.
- Treat Pan Docs documentation as the primary authority for hardware specification details.

3. Use this retrieval order when accessing Pan Docs content.

- First, try standard web retrieval of the Pan Docs page at `https://gbdev.io/pandocs/`.
- If the page cannot be retrieved with standard tools, try fetching it directly with `curl`.
- If Pan Docs still cannot be retrieved, use the raw GitHub source at `https://github.com/gbdev/pandocs` and read the relevant markdown files from `src/`.

4. When fixing a failing Mooneye acceptance test, read the test source first.

- Mooneye test source files contain precise cycle-accurate comments that Pan Docs often omits (e.g., exact M-cycle timing of register side-effects, edge cases for restarts).
- Fetch the source with `curl https://raw.githubusercontent.com/Gekkio/mooneye-test-suite/main/<path>.s`
- The path mirrors the ROM path: `acceptance/oam_dma_start.gb` → `acceptance/oam_dma_start.s`
- Inline comments like `; M=1: OAM still accessible` are authoritative — they document verified hardware observations.
- Use the test source to confirm what exact assertion the ROM makes before diagnosing the emulator.
- **When Mooneye test assertions conflict with Pan Docs, treat the Mooneye values as authoritative.** Mooneye tests are verified against real hardware. Example: Pan Docs claims CGB post-boot D=$FF E=$56, but Mooneye's boot_regs-cgb verifies D=$00 E=$08.

5. When fixing visual ROM-suite reference tests, validate suspicious reference assets before tuning.

- Read the ROM source/comments when available so the intended visual result is clear.
- If a reference PNG or framebuffer expectation looks unlike the ROM's stated output, inspect basic provenance before changing emulator behavior: compare reused CRCs across ROMs, colour counts, metadata, and whether the output can be reproduced by a native run.
- Do not force emulator output to match a reference artifact that appears to be a post-processed image, boot-screen capture, or otherwise non-hardware output. Keep the case ignored or document it as invalid until a hardware-backed/native reference is available.

6. When researching PPU Mode 3 timing penalties, apply M-cycle quantization.

- Pan Docs specifies Mode 3 penalties (OBJ penalty, SCX fine-scroll, window) in T-cycle (dot) precision.
- **Critical gap in Pan Docs**: the CPU observes Mode 3 end only at M-cycle boundaries (every 4 dots). The raw dot penalty from Pan Docs cannot be used directly as `mode3_extra_dots` — it must be quantized: `mode3_extra_dots = floor(raw_penalty_dots / 4) * 4`.
- This gap is not stated in Pan Docs but is required for cycle-accurate Mooneye tests (e.g., `intr_2_mode0_timing_sprites`) to pass. Without quantization, STAT mode reads by the CPU will be off by one M-cycle.
- When SameBoy confirms a penalty formula but your integration test still fails, check whether you need to apply this quantization before hooking the penalty into the timing engine.

7. When researching PPU FIFO/LCDC timing, identify byte/address sampling points before tuning.

- For mid-scanline LCDC changes, Pan Docs often documents the high-level effect but not the exact sub-fetch sampling point.
- After reading Pan Docs, inspect SameBoy's display/object-fetch path for where addresses and tile bytes are computed, especially whether low and high tile-data bytes recompute their address independently.
- Treat those sampling points as implementation evidence to guide tests and hypotheses before trying emulator-specific thresholds.
- For OBJ fetches that begin before x=0, separately consider whether the fetch already sampled tile bytes before the first visible pixel even though its stall affects visible pixels.

8. Account for scan-type M-cycle asymmetry when fixing LCD-enable or first-scanline timing.

- After LCD enable, scan 0 starts at **dot 4** (not dot 0). Regular scans (scan 2+) start at dot 0.
- This shifts the M-cycle grid by one: on scan 0, dot 452 = **M111**; on regular scans, dot 452 = **M112**.
- Any PPU event tied to a specific dot (early LY increment, Mode 2 STAT source, Mode 0 STAT source) fires at a **different M-cycle** on scan 0/1 vs. scan 2+. A fix that is correct for regular scans will be off by one M-cycle on the first two scans, breaking `lcdon_timing-GS`.
- When implementing dot-based timing fixes, verify the M-cycle position independently for scan 0 and for regular scans, and gate the fix with `!first_scanline_after_enable && !second_scanline_after_enable` if the behavior must only apply to scan 2+.

9. If specification coverage is missing or incomplete, inspect SameBoy carefully.

- Prefer `LIJI32/SameBoy` and focus on `Core/`.
- Use SameBoy only after checking Pan Docs and its source.
- Treat SameBoy as implementation evidence, not as equal authority with a written hardware specification.
- If SameBoy appears to make a choice where the specification is unclear, say that explicitly instead of presenting it as confirmed hardware fact.

10. When sources disagree or remain ambiguous, report that directly.

- Name the conflicting sources.
- State which source is more authoritative for the question at hand and why.
- Do not merge conflicting claims into a guessed answer.

11. Produce a detailed, source-backed answer.

- Start with a high-level explanation of the hardware behavior.
- Then cover precise details such as registers, bit meanings, address ranges, timing, ordering, side effects, open bus behavior, edge cases, and model differences.
- Clearly label what is confirmed by specification, what is supported only by emulator implementation, and what is still unknown.
- Cite the exact Pan Docs pages or SameBoy files you used.

12. Never guess.

- If no authoritative information is available, say so plainly.
- If the available information is partial, answer only the supported part and identify the gaps.

## References

- `references/source-priority.md`: source order, retrieval tips, and SameBoy lookup starting points.

## Examples

- Researching joypad register (`$FF00`) behavior:
  start with Pan Docs joypad and register pages, then follow links for timing, interrupt behavior, and model differences.

- Researching an APU channel detail:
  start with Pan Docs APU and sound controller pages, then inspect `Core/` in SameBoy only if the written specification leaves a behavior unclear.

- Researching an MBC quirk:
  start with the MBC page on Pan Docs, follow cartridge-specific links, then inspect `Core/` or related MBC files in SameBoy if the written documentation is incomplete.

- Diagnosing a failing Mooneye acceptance test (`oam_dma_start`):
  fetch the test source with `curl https://raw.githubusercontent.com/Gekkio/mooneye-test-suite/main/acceptance/oam_dma_start.s`, read the inline timing comments to understand the exact expected behavior, then cross-check Pan Docs for the register specification.
