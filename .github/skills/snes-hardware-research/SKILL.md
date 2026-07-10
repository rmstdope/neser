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

9. Use Mesen2 for ground-truth comparison and timing verification.
   - Repo: `https://github.com/SourMesen/Mesen2`
   - Highly accurate multi-system emulator with SNES support.
   - Use for pixel-perfect visual comparisons and timing verification.
   - Headless test mode: `Mesen --testRunner --enableStdout --timeout=N <rom> <script.lua>`
   - Particularly useful for PPU rendering accuracy and timing edge cases.
   - When NESER and Mesen2 disagree, investigate both against hardware specs rather than assuming either is correct.

10. When sources disagree or remain ambiguous, report that directly.
   - Name the conflicting sources.
   - State which source is more authoritative for the question at hand and why.
   - Do not merge conflicting claims into a guessed answer.

11. Produce a detailed, source-backed answer.
    - Start with a high-level explanation of the hardware behavior.
    - Then cover precise details: registers, bit meanings, address ranges, timing, ordering, side effects, open-bus behavior, edge cases, and model/region differences.
    - Clearly label what is confirmed by specification, what is supported only by emulator implementation, and what is still unknown.
    - Cite the exact fullsnes sections, anomie doc names, SNESdev wiki pages, or bsnes/higan files you consulted.

12. Never guess — especially about timing.
    - If no authoritative information is available, say so plainly.
    - If available information is partial, answer only the supported part and identify the gaps.
    - For timing-sensitive behavior: prefer Mesen2/bsnes implementation evidence over speculation, but label it clearly as "implementation-backed, not spec-confirmed."

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
- **HDMA timing and mid-scanline activation**: HDMA initialization normally occurs once per frame at the start of scanline 0 (dot 0). The per-scanline transfer point is **dot 276 (clock 1104)** on each active scanline. When HDMAEN ($420C) is written **before** dot 276 on an active scanline (common pattern: H-IRQ at dots 220-232 writing HDMAEN), newly-enabled channels must initialize and transfer on that **same scanline** — not wait until the next frame. This requires: (1) tracking mid-scanline HDMAEN writes in a pending flag, (2) initializing pending channels just before dot 276, and (3) immediately clearing active-mask bits for disabled channels. Failing to support mid-scanline activation causes test ROMs like `hdmaen_latch_test.sfc` to render blank instead of showing horizontal stripes. HDMA steals 18 + 8N + indirect-access cycles at initialization and variable cycles per scanline during transfers; general-purpose DMA pauses the CPU entirely. Both interact with PPU access windows. See issue #2943.
- **APU is asynchronous**: The SPC700 runs on its own ~1.024 MHz clock, independent of the main CPU. Port reads/writes are the only synchronization; many games rely on exact IPL upload timing.
- **VRAM access timing**: CPU access to VRAM/CGRAM/OAM is only safe during V/H-blank (or forced blank); the address-increment-on-read/write behavior of `$2116`–`$2119` is a frequent source of bugs.
- **INIDISP brightness change delay**: When INIDISP ($2100) is written mid-scanline to change brightness, the hardware has a delay (several pixels) before the change becomes visible on-screen. The exact delay timing differs between hardware revisions and is not fully documented in fullsnes. Emulators implement different delay models, causing ~4-6% pixel differences in test ROMs that hammer INIDISP mid-scanline (e.g., `inidisp_brightness_delay.sfc`). These differences concentrate at brightness transition edges and don't affect real games. See issue #2973.
- **Frame-to-frame CPU/PPU timing drift**: Emulators with imperfect CPU/PPU clock synchronization can exhibit systematic frame-to-frame timing drift where the same instruction executes at slightly different horizontal positions across frames (e.g., 2-clock shifts in a 3-frame cycle). This causes test ROMs with tight timing windows to show extra flickering lines or pixel-level visual differences from reference emulators. While low-priority for gameplay (real games have timing margins), it's detectable in test ROMs like `hdmaen_latch_test_2.sfc`. Root cause is usually cumulative rounding errors in the CPU↔PPU clock conversion or DRAM refresh position jitter. See issue #2971.
- **Copier headers**: `.smc` files may carry a 512-byte header; failing to detect/strip it offsets the entire ROM and breaks mapping detection.
- **I/O region speeds are Fast, not Slow**: B-Bus I/O (`$2000–$3FFF`) and CPU I/O (`$4200–$5FFF`) in system banks run at 3.58 MHz (6 master clocks), the same as FastROM. Only WRAM mirrors (`$0000–$1FFF`) and expansion (`$6000–$7FFF`) in system banks are slow (8 clocks). This is a common planning mistake when building cycle-accurate bus models.
- **H/V-IRQ CPU-dispatch pipeline delay**: the PPU's H/V-IRQ line (`$4211` TIMEUP bit 7) becoming true is *not* the same instant the CPU can act on it. bsnes' `CPU::irqPoll` (`sfc/cpu/irq.cpp`) samples the *previous* poll's stale line value before updating the new one, so a freshly-triggered IRQ only becomes a dispatchable "transition" (i.e. wakes `WAI` or fires the vector) on the *next* 4-clock (one-dot) poll — a fixed one-dot pipeline delay. TIMEUP register reads themselves stay instantaneous (`CPU::timeup()` reads the raw line directly); only WAI-wake/interrupt-dispatch timing has the delay. This detail is undocumented in fullsnes and was only found by reading bsnes source (`sfc/cpu/irq.cpp`, `io.cpp`, `sfc/cpu/timing.cpp`'s `stepOnce()`); see neser's `Ppu::poll_irq_dispatch`/`irq_edge_age` (landed for issue #2909/PR #2931) for a worked implementation.
- **DRAM refresh steals 40 master clocks once per scanline**: real SNES hardware pauses the CPU for 40 master clocks (10 dot-widths) per 1364-cycle scanline for WRAM refresh — fullsnes documents this in "SNES Timing H/V Counters" ("Refresh (per scanline) 40 master cycles (10 dot cycles)") but doesn't give the exact trigger clock/phase formula. Mesen2's authoritative model (`Core/SNES/SnesMemoryManager.cpp`): `_dramRefreshPosition = 538 - (_masterClock & 0x07)`, recomputed once per scanline at the scanline boundary using the CURRENT cumulative master-clock count (jittering the trigger point by up to ±7 clocks depending on total elapsed time); bsnes reaches an equivalent baseline via `status.dramRefreshPosition = 530 + 8 - dmaCounter()` (CPU version 2). This is a genuinely separate hardware feature from H/V-IRQ timing and was **completely unimplemented** in neser until issue #2930 — its absence caused a ROM-visible OPHCT/H-counter latch mismatch (off by up to a full scanline cumulatively) that looked at first like a subtle 1-clock rounding bug. See `Ppu::dram_refresh_due`/`recompute_dram_refresh_position` in `src/snes/ppu/timing.rs`. **Implementation pitfall**: DRAM refresh is a CPU/bus-wide stall, not a PPU-only event — every stolen clock must also tick the APU and input latch, or they desynchronize from the PPU's own timeline (an initial implementation that looped the extra 40 clocks *inside* `Ppu::tick()` alone passed every test that only exercised the PPU/CPU, but silently broke APU sync; caught by code review, not by tests). Model the steal at the bus level instead: tick the PPU once, query a `dram_refresh_due()`-style flag, and if set, loop the bus's *entire* per-clock sequence (APU + PPU + input) for the stolen clocks — see `SnesSystemBus::tick`/`tick_one_master_clock` in `src/snes/bus/system_bus.rs`.
- **Fixed ~186-clock CPU reset/power-on startup delay**: the 5A22 doesn't fetch its first instruction the instant RESB is released — Mesen2 models a flat, unconditional 186-master-clock delay (`SnesMemoryManager::IncMasterClockStartup`, called right after both `SnesCpu::PowerOn()` and `SnesCpu::Reset()`, i.e. applies to *both* cold power-on and any subsequent reset) before the CPU's first fetch. bsnes reaches an equivalent total via a different decomposition: 22 internal 6-clock cycles (132 clocks) consumed while `status.resetPending` is set, immediately followed by the normal (non-pushing) reset-vector interrupt dispatch sequence. This delay is not documented as a single named quantity in fullsnes and was only pinned down by comparing ground-truth `masterClock`/`scanline` values from a real Mesen2 `--testRunner` run against neser's own instruction-level trace from cold power-on. See `RESET_STARTUP_DELAY_CLOCKS` in `src/snes/cpu/cpu.rs` (landed for issue #2909/#2930).
- **Some blargg SPC/APU test ROMs require a mid-test soft reset**: e.g. `timer_at_power_reset.smc` measures behavior across both a cold power-on and a subsequent reset, and signals "I'm ready for you to press reset now" by executing `JMP $0000` into zeroed-out low WRAM (a deliberate trap distinct from the SPC700's actual reset vector) rather than via `STP`. An automated test harness must detect this and call the emulator's own `reset()`/soft-reset API to let the ROM continue into its post-reset comparison; a plain fixed-frame run will otherwise sit on a "please reset" screen forever. Mesen's default *random* WRAM power-on state (`SnesConfig::RamPowerOnState = RamState::Random`, `UI/Config/SnesConfig.cs`) was investigated as a possible ground-truth-comparison confound but was ruled out empirically (identical results with `AllZeros`/`AllOnes` forced via `settings.json`) — Mesen's `--testRunner` mode does not appear to honor that persisted setting. See `RunConfig::reset_on_pc_trap` in `src/snes/integration_tests/rom_runner.rs`.
- **Ground-truth comparison via Mesen2's `--testRunner` Lua mode**: `Mesen --testRunner --enableStdout --timeout=N <rom> <script.lua>` runs a ROM headlessly at max speed under Lua scripting until the script calls `emu.stop(exitCode)` or the timeout elapses; `--enableStdout` is required for `print()`/`emu.log()` output to reach the terminal (only `print()` was confirmed reliable). `emu.getState()` returns a **flat Lua table with dotted string keys** (e.g. `state["cpu.x"]`, `state["memoryManager.hClock"]`, `state["ppu.scanline"]`, `state["masterClock"]`) — **not** nested tables; `state.cpu.x` silently returns `nil`. Some fields are absent from the state table very early in execution, so wrap all `emu.getState()` access in `pcall()`. `emu.addMemoryCallback(callback, emu.callbackType.exec, startAddr, [endAddr], emu.cpuType.snes, emu.memType.snesMemory)` fires on instruction-fetch execution and is the most useful breakpoint-style probe for capturing register/timing state at specific PCs.

## Testing Methodology for SNES Emulation

When verifying SNES emulator accuracy:

- **Pixel-perfect comparison**: Use Python/PIL to compare screenshots pixel-by-pixel against Mesen2 captures. Don't trust visual inspection — differences of 2-4% are invisible to the eye but indicate timing bugs.
- **CRC-based integration tests**: Capture frame CRCs at known stable points (e.g., frame 600) and use as golden values for regression testing. Update test comments to reference GitHub issues for known differences.
- **Mesen2 screenshot settings**: Use `--Video.VideoFilter=None --Video.AspectRatio=NoStretching` for comparable captures (note: Mesen2 has a 1-scanline row offset vs NESER).
- **Always diff before documenting**: Before creating a "known issue" for a visual difference, run a pixel diff to quantify the discrepancy and locate where differences occur (edges, specific scanlines, etc.).
- **Example diff script**: Use PIL to iterate pixels, mark differences as red, matches as dimmed grayscale — visual diffs immediately reveal whether issues are localized (edge effects) or systematic (whole-frame shifts).


