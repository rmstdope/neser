---
name: gb-hardware-research
description: Research Game Boy/Game Boy Color hardware details. Specification authority is Pan Docs (with curl and GitHub-source fallbacks); SameBoy is the implementation reference when the specification is incomplete, Gambatte the second, and SameBoy the screenshot reference for visual test ROMs.
---

# Game Boy Hardware Research

## Introduction

Use this skill whenever you need details about any part of Game Boy or Game Boy Color hardware. This includes CPU (SM83/LR35902), PPU, APU, DMA, serial port, joypad, cartridge bus behavior, memory maps, timing, electrical quirks, MBC (memory bank controllers), and model differences (DMG, MGB, CGB, SGB). Prefer source-backed answers, be thorough, and never guess when the documentation is missing or incomplete.

## Authorities

Every hardware research skill in this repository uses the same three tiers. The tiers are ranked: a lower tier is consulted only when the tier above it does not answer the question, and an answer is always labelled with the tier it came from.

1. **Specification authority: Pan Docs** (`https://gbdev.io/pandocs/`).
   The single leading source. It decides what the hardware does. Retrieval fallbacks (`curl`, the `gbdev/pandocs` GitHub source) are transports for the same source, not different authorities. Hardware-verified test ROMs (Mooneye, blargg, the vendored visual suites) are hardware observations: when one contradicts a Pan Docs sentence, report the test ROM as authoritative for that observable and say Pan Docs disagrees (example: Pan Docs claims CGB post-boot D=$FF E=$56, Mooneye's boot_regs-cgb verifies D=$00 E=$08).

2. **Implementation references: SameBoy, then Gambatte.**
   Used only when the specification is missing, incomplete, or ambiguous, to see how a specific behavior can be implemented. SameBoy (`https://github.com/LIJI32/SameBoy`, `Core/`) is consulted first; Gambatte (`https://github.com/sinamas/gambatte`, `libgambatte/src/`) is the second, independent lineage, consulted when SameBoy has no model for the behavior or when two implementations agreeing would settle a question. Implementation evidence is never equal authority with the specification. Where an emulator makes a choice the specification does not settle, say so instead of presenting it as hardware fact.

3. **Screenshot reference: SameBoy.**
   When a visual test ROM needs a reference image, capture it with SameBoy at the same frame as NESER and pixel-diff the two captures with `python -m scripts.diff_screenshots`. An exact match approves the golden. If the captures differ and SameBoy itself is suspect, ask the navigator instead of approving either side. SameBoy's headless `sameboy_tester` (built from the repository's `Tester/`) runs a ROM for a number of seconds and writes a BMP; the verified recipe is in `scripts/reference_capture/README.md` and produced a 0-px match against NESER on dmg-acid2 on 2026-09-25. It is time-based, not frame-exact, so it can only approve screens that are static by the capture time. The upstream visual suites' own reference images are validated before use as step 5 below describes.

## Instructions

1. Define the target precisely before researching.

- Identify the hardware area, the exact behavior in question, and any model or revision constraints.
- Distinguish between questions about specification, observed behavior, emulator behavior, and MBC-specific wiring.

2. Start with Pan Docs as the specification authority.

- Look for the most specific Pan Docs page first.
- Read linked pages when the topic spans multiple components, such as CPU/PPU timing, joypad I/O, DMA interactions, or MBC-specific behavior.

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
- **When Mooneye test assertions conflict with Pan Docs, treat the Mooneye values as authoritative** and report the conflict, as the Authorities section describes.

5. When fixing visual ROM-suite reference tests, validate suspicious reference assets before tuning.

- Read the ROM source/comments when available so the intended visual result is clear.
- If the upstream suite metadata lists multiple valid result images for one ROM, preserve that as an explicit acceptance set unless hardware documentation proves a narrower model-specific mapping. Do not collapse multiple references to the one image the current emulator happens to match.
- If a reference PNG or framebuffer expectation looks unlike the ROM's stated output, inspect basic provenance before changing emulator behavior: compare reused CRCs across ROMs, colour counts, metadata, and whether the output can be reproduced by a SameBoy capture.
- Do not force emulator output to match a reference artifact that appears to be a post-processed image, boot-screen capture, or otherwise non-hardware output. Keep the case ignored or document it as invalid until a hardware-backed or SameBoy-captured reference is available.

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

9. If specification coverage is missing or incomplete, inspect SameBoy, then Gambatte.

- Check for clones alongside the current repo first (`../SameBoy`, `../gambatte`); otherwise fetch from GitHub.
- Prefer `LIJI32/SameBoy` and focus on `Core/`; entry points are listed in `references/source-priority.md`.
- Consult Gambatte (`libgambatte/src/`) only when SameBoy has no model for the behavior or a second, independent implementation is needed to settle a question. When both agree on unspecified behavior, note that explicitly; when they disagree, state both approaches and go back to Pan Docs and the test ROMs.
- Treat both as implementation evidence, not as equal authority with a written hardware specification.
- If an emulator appears to make a choice where the specification is unclear, say that explicitly instead of presenting it as confirmed hardware fact.

10. For visual verification, cross-check against SameBoy as the screenshot reference.

- Capture with `sameboy_tester --dmg|--sgb|(cgb) --length <seconds> --boot <SameBoy.app boot ROM> <rom>` as `scripts/reference_capture/README.md` describes, convert the BMP to PNG, and pixel-diff against a NESER capture of a frame past the same point with `python -m scripts.diff_screenshots <neser.png> <sameboy.png> --shift-search 1`; exact matches become the golden.
- The tester measures time in 139810-cycle blocks and NESER's boot animation is longer than SameBoy's boot ROM, so only static screens can be compared: capture at two lengths and require byte-identical BMPs before reading a diff as a NESER defect.
- Pin everything that can vary between runs on both sides (model, boot ROM, power-on RAM state; the tester disables SameBoy's randomisation itself) and never pass `--start`, which scripts button presses.
- If NESER and SameBoy disagree and the divergence is suspected to be a SameBoy quirk, ask the navigator how to proceed rather than approving either side unilaterally.
- Document the approval (frame, model, diff result) in the test comment.

11. When sources disagree or remain ambiguous, report that directly.

- Name the conflicting sources.
- State which source is more authoritative for the question at hand and why.
- Do not merge conflicting claims into a guessed answer.

12. Produce a detailed, source-backed answer.

- Start with a high-level explanation of the hardware behavior.
- Then cover precise details such as registers, bit meanings, address ranges, timing, ordering, side effects, open bus behavior, edge cases, and model differences.
- Clearly label what is confirmed by specification, what is supported only by emulator implementation, and what is still unknown.
- Cite the exact Pan Docs pages or SameBoy/Gambatte files you used.

13. Never guess.

- If no authoritative information is available, say so plainly.
- If the available information is partial, answer only the supported part and identify the gaps.

## References

- `references/source-priority.md`: source order, retrieval tips, and SameBoy/Gambatte lookup starting points.

## Examples

- Researching joypad register (`$FF00`) behavior:
  start with Pan Docs joypad and register pages, then follow links for timing, interrupt behavior, and model differences.

- Researching an APU channel detail:
  start with Pan Docs APU and sound controller pages, then inspect `Core/apu.c` in SameBoy only if the written specification leaves a behavior unclear.

- Researching an MBC quirk:
  start with the MBC page on Pan Docs, follow cartridge-specific links, then inspect `Core/mbc.c` in SameBoy if the written documentation is incomplete.

- Diagnosing a failing Mooneye acceptance test (`oam_dma_start`):
  fetch the test source with `curl https://raw.githubusercontent.com/Gekkio/mooneye-test-suite/main/acceptance/oam_dma_start.s`, read the inline timing comments to understand the exact expected behavior, then cross-check Pan Docs for the register specification.

- Approving a golden frame for a visual test ROM:
  capture the same frame in SameBoy and NESER with the same model and boot ROM, diff them with `scripts.diff_screenshots`, and record the frame and the 0-px result in the test comment.
