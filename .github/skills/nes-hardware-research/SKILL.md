---
name: nes-hardware-research
description: Research NES/Famicom hardware details from NESdev first, with curl and mirror fallbacks, and Mesen implementation only when specs are incomplete.
---

# NES Hardware Research

## Introduction

Use this skill whenever you need details about any part of NES or Famicom hardware. This includes CPU, PPU, APU, DMA, controller ports, cartridge bus behavior, memory maps, timing, electrical quirks, mappers, and console-model differences. Prefer source-backed answers, be thorough, and never guess when the documentation is missing or incomplete.

## Instructions

1. Define the target precisely before researching.
   - Identify the hardware area, the exact behavior in question, and any model or revision constraints.
   - Distinguish between questions about specification, observed behavior, emulator behavior, and board-specific wiring.

2. Start with NESdev as the primary source.
   - Look for the most specific NESdev page first.
   - Read linked pages when the topic spans multiple components, such as CPU/PPU timing, controller I/O, DMA interactions, or mapper-specific behavior.
   - Treat NESdev documentation as the primary authority for hardware specification details.

3. Use this retrieval order when accessing NESdev content.
   - First, try standard web retrieval of the NESdev page.
   - If the page cannot be retrieved with standard tools, try fetching it directly with `curl`.
   - If NESdev still cannot be retrieved, use the `nes.science` mirror, starting from `Special_AllPages.xhtml` and then opening the relevant `.xhtml` page.

4. If specification coverage is missing or incomplete, inspect Mesen carefully.
   - Prefer `SourMesen/Mesen2` and focus on `Core/NES/`.
   - Use Mesen only after checking NESdev and its mirror.
   - Treat Mesen as implementation evidence, not as equal authority with a written hardware specification.
   - If Mesen appears to make a choice where the specification is unclear, say that explicitly instead of presenting it as confirmed hardware fact.

5. When sources disagree or remain ambiguous, report that directly.
   - Name the conflicting sources.
   - State which source is more authoritative for the question at hand and why.
   - Do not merge conflicting claims into a guessed answer.

6. Produce a detailed, source-backed answer.
   - Start with a high-level explanation of the hardware behavior.
   - Then cover precise details such as registers, bit meanings, address ranges, timing, ordering, side effects, open bus behavior, edge cases, and model differences.
   - Clearly label what is confirmed by specification, what is supported only by emulator implementation, and what is still unknown.
   - Cite the exact NESdev pages or Mesen files you used.

7. Never guess.
   - If no authoritative information is available, say so plainly.
   - If the available information is partial, answer only the supported part and identify the gaps.

## References

- `references/source-priority.md`: source order, retrieval tips, and Mesen lookup starting points.

## Examples

- Researching `$4016` / `$4017` controller-port behavior:
  start with NESdev controller and register pages, then follow links for open bus, expansion-device wiring, and console-model differences.

- Researching an APU frame counter detail:
  start with NESdev APU and frame-counter pages, then inspect `Core/NES/APU/` in Mesen2 only if the written specification leaves a behavior unclear.

- Researching a mapper quirk:
  start with the mapper page on NESdev, follow board-specific links, then inspect `Core/NES/Mappers/` or related mapper factory files in Mesen2 if the written documentation is incomplete.
