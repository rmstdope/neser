---
name: nes-hardware-research
description: Research NES/Famicom hardware details. Specification authority is the NESdev wiki (with curl and mirror fallbacks); Mesen2 is the implementation reference when the specification is incomplete and the screenshot reference for visual test ROMs.
---

# NES Hardware Research

## Introduction

Use this skill whenever you need details about any part of NES or Famicom hardware. This includes CPU, PPU, APU, DMA, controller ports, cartridge bus behavior, memory maps, timing, electrical quirks, mappers, and console-model differences. Prefer source-backed answers, be thorough, and never guess when the documentation is missing or incomplete.

## Authorities

Every hardware research skill in this repository uses the same three tiers. The tiers are ranked: a lower tier is consulted only when the tier above it does not answer the question, and an answer is always labelled with the tier it came from.

1. **Specification authority: the NESdev wiki** (`https://www.nesdev.org/wiki/`).
   The single leading source. It decides what the hardware does. Retrieval fallbacks (`curl`, the nes.science mirror) are transports for the same source, not different authorities. Hardware-verified test ROMs (blargg's suites, the nes-test-roms collection) are hardware observations: when one contradicts a wiki sentence, report the test ROM as authoritative for that observable and say the wiki disagrees.

2. **Implementation reference: Mesen2** (`https://github.com/SourMesen/Mesen2`, `Core/NES/`).
   Used only when the specification is missing, incomplete, or ambiguous, to see how a specific behavior can be implemented. Implementation evidence is never equal authority with the specification. Where Mesen2 makes a choice the specification does not settle, say so instead of presenting it as hardware fact. No second implementation reference is designated for the NES; the legacy `SourMesen/Mesen` repository is history, not an authority.

3. **Screenshot reference: Mesen2.**
   When a visual test ROM needs a reference image, capture it with Mesen2 at the same frame as NESER and pixel-diff the two captures with `python -m scripts.diff_screenshots`. An exact match approves the golden. If the captures differ and Mesen2 itself is suspect, ask the navigator instead of approving either side. The verified headless recipe (Mesen2 release binary in `/Applications/Mesen.app`, testRunner mode, `scripts/reference_capture/mesen2_capture.lua`, `--nes.DisableFrameSkipping=true --nes.RamPowerOnState=AllZeros`) is in `scripts/reference_capture/README.md`; it produced a 0-px match against NESER on instr_test-v5 at frame 120 on 2026-09-25.

## Instructions

1. Define the target precisely before researching.
   - Identify the hardware area, the exact behavior in question, and any model or revision constraints.
   - Distinguish between questions about specification, observed behavior, emulator behavior, and board-specific wiring.

2. Start with NESdev as the specification authority.
   - Look for the most specific NESdev page first.
   - Read linked pages when the topic spans multiple components, such as CPU/PPU timing, controller I/O, DMA interactions, or mapper-specific behavior.

3. Use this retrieval order when accessing NESdev content.
   - First, try standard web retrieval of the NESdev page.
   - If the page cannot be retrieved with standard tools, try fetching it directly with `curl`.
   - If NESdev still cannot be retrieved, use the `nes.science` mirror, starting from `Special_AllPages.xhtml` and then opening the relevant `.xhtml` page.

4. If specification coverage is missing or incomplete, inspect Mesen2 carefully.
   - Check for a clone alongside the current repo first (`../Mesen2`); otherwise fetch from GitHub.
   - Focus on `Core/NES/`; entry points are listed in `references/source-priority.md`.
   - Use Mesen2 only after checking NESdev and its mirror.
   - Treat Mesen2 as implementation evidence, not as equal authority with a written hardware specification.
   - If Mesen2 appears to make a choice where the specification is unclear, say that explicitly instead of presenting it as confirmed hardware fact.

5. For visual verification, cross-check against Mesen2 as the screenshot reference.
   - Capture a Mesen2 screenshot at the same frame as NESER with the recipe in `scripts/reference_capture/README.md` and pixel-diff programmatically with `python -m scripts.diff_screenshots <neser.png> <mesen.png> --shift-search 1`; exact matches become the golden. Mesen2's frame count and NESER's `--frames` both count from power-on, so the same number means the same frame.
   - Capture the reference twice before trusting any non-zero diff: a capture that changes between identical runs means the reference is not pinned (power-on RAM state, frame skipping), not that NESER is wrong.
   - If NESER and Mesen2 disagree and the divergence is suspected to be a Mesen2 quirk, ask the navigator how to proceed rather than approving either side unilaterally.
   - Document the approval (frame, diff result) in the test comment.

6. When sources disagree or remain ambiguous, report that directly.
   - Name the conflicting sources.
   - State which source is more authoritative for the question at hand and why.
   - Do not merge conflicting claims into a guessed answer.

7. Produce a detailed, source-backed answer.
   - Start with a high-level explanation of the hardware behavior.
   - Then cover precise details such as registers, bit meanings, address ranges, timing, ordering, side effects, open bus behavior, edge cases, and model differences.
   - Clearly label what is confirmed by specification, what is supported only by emulator implementation, and what is still unknown.
   - Cite the exact NESdev pages or Mesen2 files you used.

8. Never guess.
   - If no authoritative information is available, say so plainly.
   - If the available information is partial, answer only the supported part and identify the gaps.

## References

- `references/source-priority.md`: source order, retrieval tips, and Mesen2 lookup starting points.

## Examples

- Researching `$4016` / `$4017` controller-port behavior:
  start with NESdev controller and register pages, then follow links for open bus, expansion-device wiring, and console-model differences.

- Researching an APU frame counter detail:
  start with NESdev APU and frame-counter pages, then inspect `Core/NES/APU/` in Mesen2 only if the written specification leaves a behavior unclear.

- Researching a mapper quirk:
  start with the mapper page on NESdev, follow board-specific links, then inspect `Core/NES/Mappers/` or related mapper factory files in Mesen2 if the written documentation is incomplete.

- Approving a golden frame for a visual test ROM:
  capture the same frame in Mesen2 and NESER, diff them with `scripts.diff_screenshots`, and record the frame and the 0-px result in the test comment.
