---
name: snes-hardware-research
description: Research Super Nintendo (SNES/Super Famicom) hardware details from fullsnes first, with anomie's docs and the SNESdev wiki as fallbacks, and ares/Mesen2 implementation evidence when specs are incomplete.
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
   - Use ares and Mesen2 source code only when specs are incomplete.

4. When researching 65816 CPU timing and cycle counts, account for variable memory speed.
   - Memory access speed depends on region (FastROM vs SlowROM, MEMSEL `$420D`) and the accessed bank/address.
   - **Fetch the `#snesmemorycontrol` anchor of fullsnes first** — it contains the authoritative per-region speed table and MEMSEL register definition.
   - The three SNES bus speeds are: **Fast 3.58 MHz (6 master clocks)**, **Slow 2.68 MHz (8 master clocks)**, **XSlow 1.78 MHz (12 master clocks)**.
   - Key regions that are commonly mis-classified: B-Bus I/O `$2000–$3FFF` and CPU I/O `$4200–$5FFF` in banks `$00–$3F`/`$80–$BF` are **Fast (6 clocks)**, not slow. Only WRAM mirrors (`$0000–$1FFF`) and expansion (`$6000–$7FFF`) are Slow (8 clocks) in those banks.
   - WS1 ROM (banks `$00–$3F`:`$8000–$FFFF`, `$40–$7D`) is **always slow (8 clocks)**; MEMSEL only affects WS2 ROM (banks `$80–$BF`:`$8000–$FFFF` and `$C0–$FF`).
   - Document cycle penalties for MMIO, WRAM, and cartridge regions separately from base instruction cycles.
   - Cross-check against anomie's timing docs and Tom Harte / ProcessorTests 65816 vectors; treat ares/Mesen2 as implementation evidence only. The vectors record **per-cycle** bus activity, not just cycle counts — see "The ProcessorTests vectors carry per-cycle bus data" below for the signal encoding, and "ProcessorTests targets a bare WDC 65816, not the 5A22" for when they lose to `cputest.sfc`.

5. When researching PPU modes and rendering, separate the many features.
   - BG modes 0–7 differ in layer count and bit depth; Mode 7 adds an affine matrix (rotation/scaling).
   - Account for windows, color math (add/subtract, half), mosaic, offset-per-tile (modes 2/4/6), hi-res/pseudo-hires (512-wide), interlace, and the OBJ (sprite) range/time over-limits.
   - Verify V/H counters, NMI, and the H/V IRQ (`$4207`–`$420A`) timing precisely.

6. When researching the APU, treat it as a separate self-contained system.
   - The SPC700 CPU + S-DSP share 64 KB ARAM; the main CPU communicates only through four I/O ports (`$2140`–`$2143` ⟷ `$F4`–`$F7`).
   - Cover the 64-byte IPL boot ROM handshake, SPC700 timers, S-DSP 8 voices, BRR block decoding, ADSR/GAIN envelopes, gaussian interpolation, echo buffer and 8-tap FIR filter.
   - Cross-check against blargg's SNES APU/SPC700 tests; treat ares/Mesen2 DSP source as implementation evidence only.

7. When researching the memory map and cartridges, verify mapping and save hardware.
   - LoROM, HiROM, ExHiROM mapping is auto-detected from the internal header (`$FFC0` area) plus heuristics; document the bank/offset math for each.
   - Battery SRAM size/presence comes from the header; document `.srm` layout expectations.
   - Note copier headers (512-byte) on `.smc` files and how to detect/strip them.

8. If specification coverage is missing or incomplete, inspect ares and Mesen2 implementation.
   - **Locating sources**: Check for cloned repositories alongside the current repo first (e.g., `../ares`, `../Mesen2`). If not found, ask the user for the location before fetching from GitHub.
   - **ares** (Near/byuu's current emulator, successor to bsnes/higan):
     - GitHub: `https://github.com/ares-emulator/ares`
     - Focus on `ares/sfc/` core for SNES implementation
     - Represents Near/byuu's latest understanding of SNES hardware
   - **Mesen2**:
     - GitHub: `https://github.com/SourMesen/Mesen2`
     - Highly accurate multi-system emulator, independent implementation
     - Source in `Core/SNES/` directory
   - Use these only after checking fullsnes, anomie's docs, and the SNESdev wiki.
   - Treat ares/Mesen2 as implementation evidence, not as equal authority with written specifications.
   - When both agree on behavior not in specs, that's strong evidence; when they disagree, state both approaches.

9. For visual verification, cross-check against Mesen2 (the sole screenshot reference; navigator decision in #3000 — ares stays a source-code reference only, see step 8).
   - **Locating the binary**:
     - Check if built from local source (e.g., `../Mesen2/bin/x64/Release/Mesen`)
     - macOS: check `/Applications/Mesen.app` (`/Applications/Mesen.app/Contents/MacOS/Mesen`)
     - Linux: check `~/Applications/`, `/usr/local/bin/`, `~/.local/bin/`
     - Windows: check `C:\Program Files\`, `C:\Program Files (x86)\`
     - If not found in standard locations, ask the user for the path
   - Capture a Mesen2 screenshot at the same frame as NESER and pixel-diff programmatically; exact matches become the reference for NESER comparison.
   - If NESER and Mesen2 disagree and the divergence is suspected to be a Mesen2 quirk, **ask the user** how to proceed rather than approving either side unilaterally.
   - Screenshot settings for comparable captures:
     - Mesen2: `--Video.VideoFilter=None --Video.AspectRatio=NoStretching --snes.disableFrameSkipping=true`
   - Mesen2 headless mode: `Mesen --testRunner --enableStdout --timeout=N <rom> <script.lua>`
   - **`--snes.disableFrameSkipping=true` is mandatory for animated content** (found in #2990):
     headless testRunner emulation runs >100 fps, engaging `_skipRender` (SnesPpu.cpp) which
     skips rendering roughly every other frame while `SendFrame` still ships the stale buffer.
     Screenshots of animated screens silently show the previous frame's pixels, producing
     phantom cadence/phase "bugs" in the reference itself. Static screens are unaffected.
     **Verify the reference capture pipeline before debugging the emulator under test.**
   - **Add `--snes.RamPowerOnState=AllZeros` whenever the ROM can display uninitialised
     WRAM** (found in #3063/#3127). NESER zero-fills WRAM; Mesen2's SNES default is
     `RamState::Random` (`SettingTypes.h`, `SnesConfig`). Without the flag the reference is
     not reproducible *against itself*, so the diff measures RNG rather than emulation.
     `test_dmatiming/demo.smc` was recorded in #3063 as a "~0.93% DMA-timing divergence"
     for exactly this reason -- two default Mesen2 captures of that ROM differ from each
     other by 1.06%, more than either differs from NESER. Matched, the real divergence was
     36 px from an unrelated cause.
     **The tell is a capture that changes between identical runs: capture twice before
     trusting any non-zero diff.** That check costs one extra run and distinguishes "the
     emulator is wrong" from "the reference is not a constant".
   - **Before reading a picture-ROM's pixel diff as a timing signal, work out what the ROM
     actually displays.** Disassembling/reading `demo.asm` showed its NMI handler is a bare
     `RTI` (confirmable in the binary: `$40` at the handler's file offset), so two of its
     three on-screen rows render never-written WRAM and only the third is a measurement.
     That reframed a whole-screen CRC into a two-word assertion on `$7EC000`/`$7EC002`,
     which is immune to power-on state and names the actual defect. Prefer extracting the
     ROM's own measurement words over hashing its framebuffer.

10. When sources disagree or remain ambiguous, report that directly.
   - Name the conflicting sources.
   - State which source is more authoritative for the question at hand and why.
   - Do not merge conflicting claims into a guessed answer.

11. Produce a detailed, source-backed answer.
    - Start with a high-level explanation of the hardware behavior.
    - Then cover precise details: registers, bit meanings, address ranges, timing, ordering, side effects, open-bus behavior, edge cases, and model/region differences.
    - Clearly label what is confirmed by specification, what is supported only by emulator implementation, and what is still unknown.
    - Cite the exact fullsnes sections, anomie doc names, SNESdev wiki pages, or ares/Mesen2 files you consulted.

12. Never guess — especially about timing.
    - If no authoritative information is available, say so plainly.
    - If available information is partial, answer only the supported part and identify the gaps.
    - For timing-sensitive behavior: prefer ares/Mesen2 implementation evidence over speculation, but label it clearly as "implementation-backed, not spec-confirmed."
    - When ares and Mesen2 agree on unspecified behavior, note that explicitly as "both ares and Mesen2 implement X."

## References

- `references/source-priority.md`: source order, retrieval tips, and ares/Mesen2 lookup starting points.

## Examples

- Researching the memory map / LoROM vs HiROM mapping:
  start with the fullsnes memory map section, cross-check anomie's memory-map doc, then ares `ares/sfc/cartridge/` and Mesen2 `Core/SNES/Cartridge.cpp` only if heuristics remain unclear.

- Researching Mode 7 rendering:
  start with the fullsnes PPU section and anomie's PPU doc for the affine matrix math, then inspect ares `ares/sfc/ppu/` and Mesen2 `Core/SNES/SnesPpu.cpp` if per-scanline edge cases remain unclear.

- Researching the APU I/O port handshake:
  start with fullsnes APU section for the IPL boot protocol, cross-check anomie's APU/DSP docs, then verify against blargg's SPC700 tests and ares `ares/sfc/smp/` / `ares/sfc/dsp/` and Mesen2 `Core/SNES/Apu/`.

- Researching 65816 instruction timing:
  start with anomie's timing doc and fullsnes CPU section, then cross-check Tom Harte / ProcessorTests 65816 vectors and ares `ares/component/processor/wdc65816/` and Mesen2 `Core/SNES/SnesCpu.cpp`.

## Known Hardware Gotchas

When writing code that targets or emulates SNES hardware, always verify against these known pitfalls:

- **65816 mode flags**: The M (accumulator) and X (index) width flags change register sizes at runtime; emulation mode (E) forces 8-bit and re-maps the stack to page 1. Misinterpreting flag state corrupts decode width and cycle counts.
- **Open bus / MDR**: Reads of unmapped or write-only addresses return the last value on the memory data register (open bus), not zero. This also applies to *unused bits inside readable registers* (fullsnes "Unused bits" table): $4210 RDNMI bits 6-4, $4211 TIMEUP bits 6-0, $4212 HVBJOY bits 5-1, $4016 bits 7-2, $4017 bits 7-5 all read the MDR. Real code depends on it: PeterLemon's `WaitNMI` macro is `bit.w $4210` / `bpl`, where the operand high byte $42 is the last fetch before the data read, so RDNMI bit 6 reads 1 and `BIT` leaves V=1 after the loop (CPUPHL.sfc fails without this; see issue #2975).
- **HDMA timing and mid-scanline activation**: HDMA initialization normally occurs once per frame at the start of scanline 0 (dot 0). The per-scanline transfer point is **dot 276 (clock 1104)** on each active scanline. When HDMAEN ($420C) is written **before** dot 276 on an active scanline (common pattern: H-IRQ at dots 220-232 writing HDMAEN), newly-enabled channels must initialize and transfer on that **same scanline** — not wait until the next frame. This requires: (1) tracking mid-scanline HDMAEN writes in a pending flag, (2) initializing pending channels just before dot 276, and (3) immediately clearing active-mask bits for disabled channels. Failing to support mid-scanline activation causes test ROMs like `hdmaen_latch_test.sfc` to render blank instead of showing horizontal stripes. HDMA steals 18 + 8N + indirect-access cycles at initialization and variable cycles per scanline during transfers; general-purpose DMA pauses the CPU entirely. Both interact with PPU access windows. See issue #2943.
  **fullsnes says H=278, and that is not a conflict** (settled in #3083): its schedule table
  gives `H=6, V=0 reload HDMA registers` and `H=278, V=0..224 perform HDMA transfers`. The V
  range matches NESER and Mesen2 exactly. The H figures differ only because dot 276 is when the
  transfer is *armed* — the first B-bus write does not land until the start delay,
  `SyncStartDma` pad, 8-clock overhead and the slot's first 4 clocks are paid, around dot 280.
  Don't "correct" the constant to 278; check which instant each source is naming first.
- **APU is asynchronous**: The SPC700 runs on its own ~1.024 MHz clock, independent of the main CPU. Port reads/writes are the only synchronization; many games rely on exact IPL upload timing.
- **VRAM access timing**: CPU access to VRAM/CGRAM/OAM is only safe during V/H-blank (or forced blank); the address-increment-on-read/write behavior of `$2116`–`$2119` is a frequent source of bugs.
- **INIDISP brightness change delay**: When INIDISP ($2100) is written mid-scanline to change brightness, the hardware has a delay (several pixels) before the change becomes visible on-screen. The exact delay timing differs between hardware revisions and is not fully documented in fullsnes. Emulators implement different delay models, causing ~4-6% pixel differences in test ROMs that hammer INIDISP mid-scanline (e.g., `inidisp_brightness_delay.sfc`). These differences concentrate at brightness transition edges and don't affect real games. See issue #2973.
- **BG vertical scroll is display-line based, off by one from the framebuffer row**: the BG tile fetch adds BGnVOFS to the raw display line (vcounter), and the first visible line is vcounter 1 — line 0 is never rendered — so framebuffer row `y` samples BG line `y + 1 + VOFS`. This is why games routinely write VOFS = -1 (0x3FF) to pixel-align a BG with the top of the screen. Confirmed in ares (`background.cpp` fetchNameTable: `voffset = vcounter() + vscroll`, early-return at vcounter 0) and Mesen2 (`SnesPpu.cpp`: `realY = _scanline`, renders scanlines 1..224). The same +1 applies to the offset-per-tile voffset paths and to Mode 7 (`realY = _scanline` there too). OBJ/sprites do NOT get the +1 in framebuffer-row space: evaluation happens one line early, so a sprite with OAM Y=k occupies framebuffer rows k..k+h-1 — a renderer indexed by 0-based output row needs +1 for BG/Mode 7 but not for OBJ. NESER got this wrong for tile BGs (while Mode 7 was correct) until issue #2945; the symptom was every BG one row too low, masked in Mesen2 comparisons as an apparent "1-row capture offset". See `effective_offsets`/`screen_y` in `src/snes/ppu/background.rs`.
- **Frame-to-frame CPU/PPU timing drift**: Emulators with imperfect CPU/PPU clock synchronization can exhibit systematic frame-to-frame timing drift where the same instruction executes at slightly different horizontal positions across frames (e.g., 2-clock shifts in a 3-frame cycle). This causes test ROMs with tight timing windows to show extra flickering lines or pixel-level visual differences from reference emulators. While low-priority for gameplay (real games have timing margins), it's detectable in test ROMs like `hdmaen_latch_test_2.sfc`. Root cause is usually cumulative rounding errors in the CPU↔PPU clock conversion or DRAM refresh position jitter. See issue #2971.
- **Copier headers**: `.smc` files may carry a 512-byte header; failing to detect/strip it offsets the entire ROM and breaks mapping detection.
- **I/O region speeds are Fast, not Slow**: B-Bus I/O (`$2000–$3FFF`) and CPU I/O (`$4200–$5FFF`) in system banks run at 3.58 MHz (6 master clocks), the same as FastROM. Only WRAM mirrors (`$0000–$1FFF`) and expansion (`$6000–$7FFF`) in system banks are slow (8 clocks). This is a common planning mistake when building cycle-accurate bus models.
- **The CPU samples the IRQ line at the START of each cycle, and it is three signals, not one**: dispatch, WAI's wake, and the `IdleOrRead` conversion each read the line at a different point, and collapsing any two of them breaks a ROM. Mesen2's `ProcessCpuCycle` -> `DetectNmiSignalEdge` (`Core/SNES/SnesCpu.cpp:124`) runs **before** the cycle's clocks, so `CheckForInterrupts` dispatches off `PrevIrqSource` = the line as of the start of the instruction's *final* cycle; a line rising mid-cycle waits for the next boundary. byuu's `test_irq.asm` sub-tests 1/2 are a deliberate 2-clock bracket around exactly this: with the trigger 2 clocks into `sec`'s final cycle hardware finishes `nop` too and pushes `clc`'s address (`$18`), and seeking 2 clocks later flips it to `$EA`. Sampling at cycle *end* instead dispatches one instruction early — neser's bug until #3146, and it broke all ten sub-tests, not just the one the ROM reports. Two things ride alongside: WAI's wake keeps the **end**-of-cycle sample (Mesen2's `_waiOver`; moving it too costs an extra idle and repaints `inidisp_forgot_to_force_blank.sfc`), and `step()` must read the dispatch sample **after** the WAI wake cycles, since those re-sample it and on the waking iteration the line typically rises during exactly them — Mesen2 does this by calling `CheckForInterrupts()` after `ProcessHaltedState`'s `Idle()`. Separately, `SnesCpu::IdleOrRead` (`SnesCpu.Shared.h:386`) turns an implied/accumulator instruction's internal cycle into a dummy read at PC when an interrupt is imminent — 8 master clocks in slow ROM against the idle's 6 — for the 33 opcodes Mesen2 routes through `AddrMode_Imp`/`AddrMode_Acc`. See `Cpu::resample_irq_line`/`resample_irq_wai_line`/`tick_idle_or_dummy_read` (#3146).
- **When byuu's ROMs and a Mesen2 golden disagree, trace-diff immediately — do not theorise**: #3146's dispatch fix made `test_irq.smc` pass and regressed `hdmaen_latch_test.sfc`'s Mesen2-exact golden by 51 scanlines. Two plausible mechanisms were proposed and both were falsified by measurement: the missing `IdleOrRead` (ported it; the ROM's output was *identical*, same CRC, same pixel diff) and the DMA lock over-suppressing the sample (one probe: `dma_locked=0` throughout). The bus-trace diff then localised it in one run — the emulators matched bit-for-bit for 1327 ordinals then split on a single dispatch decision, NESER pushing `$80B7` where Mesen2 pushed `$80B5`. The toolchain for this is committed and costs minutes (`NESER_TRACE_CPU=2` + `NESER_BUS_LOG=1` + `scripts/diff_bus_traces.py`); use it to *choose* between candidate mechanisms rather than to confirm one you already believe. Two cheap habits that paid off here: check the ROM's own annotations first (byuu documents `sec`'s per-cycle H/V positions, which excluded a ~300k-clock counted-seek confound outright), and baseline every golden you might move *before* changing anything.
- **One counter circuit drives BOTH NMI and H/V-IRQ**: the S-CPU generates the VBlank NMI from the same master-clock/4 circuit as the H/V-IRQ, so both signals share one derivation of the intra-line clocks — Mesen2 puts both in `InternalRegisters::ProcessIrqCounters` despite the name, and since #3145 neser does too (`Ppu::process_irq_counters`, `src/snes/ppu/irq.rs`). The NMI half is two arms: at intra-line clock 2 the RDNMI (`$4210` bit 7) flag rises on the first vblank scanline and falls on scanline 0 (fused with the V-counter reset, which is safe only because the vblank line is 225/240 and never 0); at clock 6 the CPU-facing NMI line follows. That 4-clock gap **is** the `$4210` read-hold window — a read landing in it returns the flag set without acknowledging, which is what lets a tight RDNMI poll loop see the same vblank twice (Terranigma sprite corruption without it; anomie's timing.txt phrases the clock-2 event as "the internal timer will set its NMI output low at H=0.5"). Two places where neser deliberately differs from Mesen2 here, neither distinguished by any ROM we have, both worth knowing before chasing an NMI timing bug: (1) the level `nmi_enable && nmi_flag` rises at clock 2, but neser only *samples* it at clock 6 via `update_nmi_line`'s rising-edge check, where Mesen2 arms unconditionally with `SetNmiFlag(1)` — so any non-disabling `$4200` write landing in clocks 2-5 samples the level first and leaves clock 6 with nothing rising to find; (2) neser derives the NMI scanline live from SETINI (`vblank_start_line()`) where Mesen2 latches `_nmiScanline` once per frame in `SnesPpu::UpdateNmiScanline`, so any mid-frame overscan toggle moves neser's NMI scanline (and, since the hold window is derived the same way, can remove that window mid-line).
- **H/V-IRQ is a level+edge counter circuit, not a trigger instant**: since #3144 neser models the H/V-IRQ as a line-for-line port of Mesen2's schematic-derived circuit (`Core/SNES/InternalRegisters.h`, `UpdateIrqLevel`/`ProcessIrqCounters`/`SetIrqFlag`; see `src/snes/ppu/irq.rs`): dedicated H/V counters ticked every 4 master clocks (intra-line clocks 2, 6, 10, ...), a continuous compare level against HTIME/VTIME, and a `need_irq` countdown armed on the level's rising edge that sets TIMEUP (`$4211` bit 7) one tick before raising the CPU-facing line. The one-dot gap between a TIMEUP read seeing the flag and the CPU being able to dispatch (previously modelled as the bsnes `CPU::irqPoll` stale-sample delay via the now-removed `irq_edge_age`) falls out of that countdown. Semantics the point-compare model could not express, all measured by byuu's `test_irq.asm`/`test_irq4200.asm` (vendored, with SRAM-verdict transcriptions in `kungfufurby_irq_tests.rs`): `$4211` reads within 4 clocks of the flag rising do NOT acknowledge; `$4207-$420A` writes re-derive the level at write time; a `$4200` enable while the compare already holds fires an IRQ on the next circuit tick. Two source caveats worth remembering: byuu's `test_irq.asm` header claim for H-enabled HTIME=0 (d7 at H=10) is unverified extrapolation — no ROM sub-test probes it, and Mesen2's 3-tick countdown (d7 at 14, dispatch at 18) is what passes the suite; and the circuit shares Mesen2's power-on artifact (the H counter's very first clock-2 increment makes HTIME=1 fire once spuriously on scanline 0), which unit tests must cross before measuring steady-state clocks.
- **DRAM refresh steals 40 master clocks once per scanline**: real SNES hardware pauses the CPU for 40 master clocks (10 dot-widths) per 1364-cycle scanline for WRAM refresh — fullsnes documents this in "SNES Timing H/V Counters" ("Refresh (per scanline) 40 master cycles (10 dot cycles)") but doesn't give the exact trigger clock/phase formula. Mesen2's authoritative model (`Core/SNES/SnesMemoryManager.cpp`): `_dramRefreshPosition = 538 - (_masterClock & 0x07)`, recomputed once per scanline at the scanline boundary using the CURRENT cumulative master-clock count (jittering the trigger point by up to ±7 clocks depending on total elapsed time); bsnes reaches an equivalent baseline via `status.dramRefreshPosition = 530 + 8 - dmaCounter()` (CPU version 2). This is a genuinely separate hardware feature from H/V-IRQ timing and was **completely unimplemented** in neser until issue #2930 — its absence caused a ROM-visible OPHCT/H-counter latch mismatch (off by up to a full scanline cumulatively) that looked at first like a subtle 1-clock rounding bug. See `Ppu::dram_refresh_due`/`recompute_dram_refresh_position` in `src/snes/ppu/timing.rs`. **Implementation pitfall**: DRAM refresh is a CPU/bus-wide stall, not a PPU-only event — every stolen clock must also tick the APU and input latch, or they desynchronize from the PPU's own timeline (an initial implementation that looped the extra 40 clocks *inside* `Ppu::tick()` alone passed every test that only exercised the PPU/CPU, but silently broke APU sync; caught by code review, not by tests). Model the steal at the bus level instead: tick the PPU once, query a `dram_refresh_due()`-style flag, and if set, loop the bus's *entire* per-clock sequence (APU + PPU + input) for the stolen clocks — see `SnesSystemBus::tick`/`tick_one_master_clock` in `src/snes/bus/system_bus.rs`.
- **Fixed ~186-clock CPU reset/power-on startup delay**: the 5A22 doesn't fetch its first instruction the instant RESB is released — Mesen2 models a flat, unconditional 186-master-clock delay (`SnesMemoryManager::IncMasterClockStartup`, called right after both `SnesCpu::PowerOn()` and `SnesCpu::Reset()`, i.e. applies to *both* cold power-on and any subsequent reset) before the CPU's first fetch. bsnes reaches an equivalent total via a different decomposition: 22 internal 6-clock cycles (132 clocks) consumed while `status.resetPending` is set, immediately followed by the normal (non-pushing) reset-vector interrupt dispatch sequence. This delay is not documented as a single named quantity in fullsnes and was only pinned down by comparing ground-truth `masterClock`/`scanline` values from a real Mesen2 `--testRunner` run against neser's own instruction-level trace from cold power-on. See `RESET_STARTUP_DELAY_CLOCKS` in `src/snes/cpu/cpu.rs` (landed for issue #2909/#2930).
- **Some blargg SPC/APU test ROMs require a mid-test soft reset**: e.g. `timer_at_power_reset.smc` measures behavior across both a cold power-on and a subsequent reset, and signals "I'm ready for you to press reset now" by executing `JMP $0000` into zeroed-out low WRAM (a deliberate trap distinct from the SPC700's actual reset vector) rather than via `STP`. An automated test harness must detect this and call the emulator's own `reset()`/soft-reset API to let the ROM continue into its post-reset comparison; a plain fixed-frame run will otherwise sit on a "please reset" screen forever. Mesen's default *random* WRAM power-on state (`SnesConfig::RamPowerOnState = RamState::Random`, `UI/Config/SnesConfig.cs`) was investigated as a possible ground-truth-comparison confound but was ruled out empirically (identical results with `AllZeros`/`AllOnes` forced via `settings.json`) — Mesen's `--testRunner` mode does not appear to honor that persisted setting. See `RunConfig::reset_on_pc_trap` in `src/snes/integration_tests/rom_runner.rs`.
- **Ground-truth comparison via Mesen2's `--testRunner` Lua mode**: `Mesen --testRunner --enableStdout --timeout=N <rom> <script.lua>` runs a ROM headlessly at max speed under Lua scripting until the script calls `emu.stop(exitCode)` or the timeout elapses; `--enableStdout` is required for `print()`/`emu.log()` output to reach the terminal (only `print()` was confirmed reliable). `emu.getState()` returns a **flat Lua table with dotted string keys** (e.g. `state["cpu.x"]`, `state["memoryManager.hClock"]`, `state["ppu.scanline"]`, `state["masterClock"]`) — **not** nested tables; `state.cpu.x` silently returns `nil`. Some fields are absent from the state table very early in execution, so wrap all `emu.getState()` access in `pcall()`. `emu.addMemoryCallback(callback, emu.callbackType.exec, startAddr, [endAddr], emu.cpuType.snes, emu.memType.snesMemory)` fires on instruction-fetch execution and is the most useful breakpoint-style probe for capturing register/timing state at specific PCs.
- **SA-1 BW-RAM protection comparator operates on the pre-wrap bus address, folded to 256KB**: the `$2228` BWPA comparator sits on the address bus and sees the *linear* BW-RAM offset folded to the 256KB address space (bsnes/ares `bwram.cpp`: `(address & 0x3ffff) < 0x100 << bwp`) — wrapping onto a smaller physical chip happens *afterward*, at the RAM's own address pins. Two consequences that a plausible-sounding "check protection against the wrapped physical offset" model gets wrong (a Copilot review on PR #2965 proposed exactly that and it was wrongly confirmed; absindx `SA1RamProtectionTest` TEST IDs 50/51 caught it in #2962): a mirrored write addressed beyond the protected linear range *succeeds* and physically lands on a protected byte via chip wraparound, while bank mirrors above 256KB fold back *inside* the protected range (so BWPA >= `$0A` protects everything). Related per-side asymmetries found in the same conformance run: the SA-1-side direct BW-RAM window spans banks `$40-$5F`, twice the SNES side's `$40-$4F` (bsnes `SA1::read` dispatch; the ROM's mirror-mask expectations for TEST IDs 155-162 encode this), and releasing SA-1 from reset clears the SA-1-side I-RAM protection register CIWP (`$222A`) but not the SNES-side SIWP (bsnes `writeIOCPU` case `$2200`: "CIWP is set to 0 at reset"; TEST ID 221). SA-1-side *open bus* remains unknown spec: the absindx author documents it as unresolved, and bsnes/ares return `$FF` for unhandled SA-1-side IO reads with an explicit "unverified" comment (neser returns `$00`) — don't treat either as hardware truth.
- **Frame numbering must count every vblank, including those inside one CPU step**: a
  "frame complete" bool consumed at instruction boundaries silently swallows vblanks that
  elapse while the CPU is stalled inside a single instruction span — a 64KB DMA is ~1.5
  frames, so an init sequence with several big DMA clears loses multiple frames and every
  animated capture lands at a constant offset from Mesen2's per-vblank `_frameCount` even
  though the emulated timing agrees. Model it as a pending counter drained per step (see
  `Ppu::take_completed_frames`, issue #2990). Static-settle goldens cannot detect this —
  a settled screen matches at any frame offset — so only animated content exposes it.
- **RDNMI ($4210) sub-scanline timing and the read-hold window**: the vblank flag rises at
  intra-line clock 2 of the first vblank scanline (anomie timing.txt: NMI output asserted
  at H=0.5) and falls at clock 2 of scanline 0; the CPU NMI line rises 4 clocks later at
  clock 6; and a $4210 read landing in the clock 2-5 window returns bit 7 set WITHOUT
  acknowledging the flag (Mesen2 `InternalRegisters::Read`, hardware-verified via
  Terranigma sprite corruption; not documented in fullsnes/SNESdev). A tight
  `bit $4210 / bpl` poll loop whose read phases through that window observes the same
  vblank twice, producing a stable alternating double-step cadence (PeterLemon scroll
  demos advance +2,+1 per frame pair). Implemented for #2990 in
  `Ppu::evaluate_nmi_flag_events` / the `$4210` read arm.
- **Clock-stamped trace-diff bisection between NESER and Mesen2**: to localize a timing
  divergence, stamp the same observable events in both emulators with their master-clock
  counters and align to the first divergent event. Mesen2 side: `--testRunner` Lua with
  `emu.addMemoryCallback` on register writes (watch BOTH bank $00 and the $80 mirror —
  FastROM code writes via $80xxxx and a bank-$00-only callback silently misses them) and
  `emu.getState()["masterClock"]`. NESER side: temporary env-gated `eprintln!` traces at
  the same registers with `total_master_clocks`. In #2990 this aligned both emulators
  byte-for-byte up to the first MDMAEN write and exposed each 64KB DMA completing ~15,800
  clocks early (= ~390 crossed scanlines x 40 unpaid DRAM-refresh clocks, issue #2985) in
  two trace runs.
- **Mode 5/6 true-hires compositing quirks are Mesen2-source-only territory** (from
  #3016): fullsnes/anomie describe hires as "512-wide, sub screen on the left
  half-pixel" and stop there — every load-bearing detail below is absent from the
  written specs and was pinned by reading Mesen2 source line-by-line
  (`SnesPpu.cpp` RenderTilemap / GetTilemapData / ApplyColorMath / RenderBgColor)
  and locking each formula with a unit test before implementing:
  the sub screen supplies the EVEN output column and main the ODD (ApplyHiResMode
  `out[2x] = sub`); BGnHOFS applies in half-pixel units with every tilemap entry a
  forced 16-wide char N/N+1 pair (hflip mirrors across the pair; the BGMODE
  tile-size bit only pairs vertically); mode 6 offset-per-tile is HALVED — the H
  value is ORed into the doubled scroll's bits 3-9 then shifted back down, so the
  entry column moves by `(value & 0x3F0) >> 4` and value bit 3 is dropped, while
  fine x/char-half always follow BGnHOFS; mosaic on a hires layer collapses each
  half-pixel pair to the even block-start sample even at block size 1; CGWSEL bit
  1 does NOT gate the sub screen's hires display (it only selects the math
  operand), and the DISPLAYED sub backdrop is CGRAM color 0 (RenderBgColor fills
  both buffers from `cgram[0]`) even though the math-operand fallback is COLDATA;
  hires color math runs per half-pixel — the odd/main half against the PRE-math
  sub color at the same x, the even/sub half against the POST-math main pixel at
  x-1 with the CGADSUB gate following the main source at x-1 (Mesen2 passes
  `prevX`), window tests at the current x for both halves. For any future PPU
  compositing work in this area, go straight to Mesen2 source verification with
  per-formula unit tests; the spec docs will not warn you.
- **Screen-interlace field content is mode 5/6-only, with alternating field lengths**
  (from #3017): Mesen2's `IsDoubleHeight() = ScreenInterlace && (BgMode == 5 || 6)` —
  only those modes fetch a doubled field line (`realY = (scanline << 1) | oddFrame`,
  GetTilemapData/GetChrData); in modes 0-4/7 + interlace both fields legitimately
  render IDENTICAL content and only the output row parity alternates
  (`row = scanline*2 + oddFrame`). Don't "fix" non-hires interlace to differ per
  field — that diverges from the reference. Supporting behaviors that ARE part of
  the model: interlaced even fields ($213F.7 = 0) run one extra scanline (263
  NTSC / 313 PAL), latched once per frame right after the field toggle
  (UpdateNmiScanline) so mid-frame SETINI writes never retime the frame in
  progress, and the non-interlace scanline-240 short-line rule is suppressed;
  under IsDoubleHeight the mosaic block-hold subtraction doubles with an
  asymmetry — the tilemap fetch keeps the field term but the CHR fetch also
  subtracts it (GetTilemapData :177-182 vs GetChrData :223-228), so the chr row
  is field-independent while the map row can differ per field; enabling
  interlace during vblank memsets the output buffer ($2133 handler); and the
  previous field's rows persist into the next frame's output (Mesen2 memcpys the
  prior interlaced frame forward), so captures weave frames N-1 and N — verify
  FIELD PARITY alignment when pixel-diffing (a mismatch shows as globally
  swapped row parity). Sprites never use screen interlace for fetch (only SETINI
  bit 1 OBJ interlace); Mode 7 has no field term at all.
- **Mid-scanline register-write effects are about WRITE clocks, not render
  clocks — HDMA B-bus writes land well after the dot-276 trigger** (from #3020):
  Mesen2 renders lazily in chunks flushed at the top of EVERY `SnesPpu::Write`
  (`RenderScanline` draws up to `x = min(hClock/4 − 22, 255)` with the OLD
  state, then the new value is stored), and its HDMA event at hClock 1104 only
  *queues* the transfer — start delay + `SyncStartDma` + `IncMasterClock8` +
  4+4 clocks per byte mean the first B-bus write reaches a register at
  >= ~hClock 1122, after the last visible pixel (x = 255, drawable at 1108) is
  flushed. An emulator that applies the whole burst atomically at the trigger
  makes x = 255 see the NEXT line's values — the signature is a divergence
  confined to the rightmost output column of HDMA-driven scenes (Perspective's
  Mode 7 matrix, InterlaceSimpsonsHDMA's BG12NBA, undisbeliever hdmaen_latch).
  In a per-dot renderer the fix is to move the WRITES (schedule each B-bus
  write at its modeled per-byte bus clock and drain after the PPU tick), not
  to touch the renderer. Residual caveat: for bursts long enough to cross the
  scanline wrap (8-channel hvdma), pixel-exactness additionally needs Mesen2's
  CPU-alignment jitter on the write clocks, which a fixed-offset model without
  a halting CPU can only approximate (#3042).
- **65816 IRQ recognition uses the I flag from BEFORE a flag-write instruction's
  effect lands** (from #2985): the interrupt logic polls per cycle with the
  previous cycle's flags (Mesen2 `DetectNmiSignalEdge` -> `PrevIrqSource`,
  consumed by `CheckForInterrupts` after each instruction). Consequences:
  CLI/SEI/PLP/REP/SEP change IRQ dispatchability only after the FOLLOWING
  instruction executes (their flag write lands after the recognition poll),
  while RTI's mid-instruction P pull IS visible on its remaining cycles
  (dispatch immediately after RTI), as are BRK/COP/hardware-dispatch I-sets.
  Real code depends on it: the `CLI ; RTI` epilogue idiom (absindx
  SA1RamProtectionTest's message wrapper) relies on the pending IRQ dispatching
  only AFTER the full RTI unwind at the original interrupted context — an
  emulator sampling the live flag dispatches between the CLI and RTI, nesting
  interrupt frames hardware never creates and (in that ROM) re-executing the
  handler with stale state. fullsnes does not document this; Mesen2's
  SnesCpu.cpp/SnesCpu.Shared.h is the reference. See `Cpu::irq_i_shadow` in
  src/snes/cpu/cpu.rs for NESER's per-instruction port.
- **Trace-diff practicalities for cross-CPU (SA-1) bugs** (from #2985): (1) both
  cores share `cpu.rs`, so temporary dispatch/RTI eprintln hooks interleave both
  CPUs' lines — log the stack pointer to disambiguate (SNES stack lives in
  `$01xx`, the SA-1's in I-RAM `$3xxx`). (2) When a traced control flow seems
  impossible (handlers running twice without dispatch, RTIs from nowhere), dump
  the actual ROM BYTES at the anomalous PC immediately (LoROM: file offset =
  `addr - $8000` per 32KB bank) instead of theorizing from the .asm sources —
  the decisive $81F1=CLI/$81F2=RTI discovery took one hexdump after several
  wasted deduction rounds. (3) A synchronous GPDMA in a bus test ticks the PPU
  past scanline 0's HDMA-init point (see the auto-memory note). the ROMs' `org $0000` result variables (`TestFinished`: 0=Running, 1=Passed, 255=Failed) are accessed by the SNES CPU via direct page with D=0, i.e. they live in **WRAM `$7E0000`** — polling the SA-1 I-RAM mirror at `$003000` instead reads a stale `$AA` left over from the I-RAM mirroring sub-tests. Even at the right address, a naive every-tick poll false-fails: `SA1RamProtectionTest` transiently `EOR #$FF`s its own result byte while probing I-RAM writability, and its SA-1 CPU parks in its post-test idle loop *before* the SNES finishes the trailing `TestBwRamSize` scan and writes the real result. `SA1VersionCodeTest` never reports Passed **even on real hardware** — disassembly of its release build shows `CheckResult` unconditionally taking the failed path (the `INC TestFinished` pass path at `$9EAB` is unreferenced dead code), deliberate since the true version-code value is unknown; treat its FAILED register-dump screen as the expected result. Its open-bus detection scheme is worth knowing: each register is read twice with different residual bus bytes (`LDA $2300,X` leaves operand `$23`; a `REP`-adjusted `LDA $AA,X` reaching the same register leaves `$AA`), so open-bus entries echo `23`/`AA` while real registers repeat their value. Both ROMs misbehave on Mesen2 (documented upstream and confirmed), so goldens must be navigator-approved captures of the emulator's own rendering.

- **DMA/HDMA transfers are ARMED at their trigger and RUN a CPU cycle later, and
  the two moments read different state** (from #3021, Mesen2 `SnesDmaController`):
  `BeginHdmaTransfer` sets the pending flag **only if HDMAEN is non-zero at the
  dot-276 trigger clock**, and also sets `_dmaStartDelay`; `ProcessPendingTransfers`
  then burns one CPU cycle on the delay and runs the burst at the start of the
  **second** CPU cycle after the trigger -- where `ProcessHdmaChannels` re-reads
  HDMAEN to decide which channels participate. So a ROM enabling a channel
  mid-scanline *after* the trigger must wait for the next line even though the
  run itself sees the new value. The same arm-then-delay shape applies to `$420B`
  general-purpose DMA. Getting the gate wrong (arming unconditionally) makes an
  emulator run HDMA lines hardware skips; getting the delay wrong shifts every
  B-bus write by 8-16 clocks. Both are visible in `hdmaen_latch_test*.sfc`.
- **The rest of the envelope**: `SyncStartDma` pads 1-8 clocks to the next
  8-master-clock boundary and *resets* the clock counter; 8 clocks of setup; 8
  per channel and 8 per byte (a byte slot is 4 clocks to the A-bus read then 4
  more before the B-bus write, so writes land at the slot's **end**);
  `SyncEndDma` pads `cpuSpeed - (counter % cpuSpeed)` to a whole CPU cycle.
  HDMA's per-line work is two-phase -- all transfers first, then per-channel
  bookkeeping with a **speculative descriptor read every line** (8 clocks; the
  table pointer advances only when the line counter expires), indirect pointer
  loads **before** the termination check, and a last-active-channel oddity where
  a `$00` descriptor loads only one pointer byte (8 clocks, not 16) into the
  high byte. Modelling this as a flat per-line constant is what #3021 replaced.

## Testing Methodology for SNES Emulation

When verifying SNES emulator accuracy:

- **Mesen2 verification (mandatory for screenshots)**:
  1. Capture a Mesen2 screenshot for the same ROM/frame (Mesen2 is the sole
     screenshot reference; ares is source-code evidence only — navigator
     decision in #3000)
  2. Pixel-diff NESER vs Mesen2 programmatically; an exact match approves the
     baseline
  3. If they differ and Mesen2 itself is suspect: **ask the user** how to
     proceed — do not approve either side arbitrarily
  4. Document the Mesen2 approval (frame, diff result) in test comments
- **Pixel-perfect comparison**: use the committed tool, don't write another one.

  ```bash
  python -m scripts.diff_screenshots neser.png mesen.png --shift-search 1 [--rows] [--out diff.png]
  ```

  Exits non-zero unless the images are pixel-identical, and stays non-zero even when some
  offset lines them up, because a non-zero best shift is a bug and not a convention.
  `--rows` prints each image's per-row mean-luminance vector plus the lag that best aligns
  them — the diagnostic for ROMs that band by scanline, whose whole signal a 2D shift
  search cannot express. `--out` writes the red-marked diff image.

  Committed in #3083 precisely because this step had only ever existed as prose: every
  investigation re-wrote a throwaway PIL script, and that is how the retracted "harmless
  constant 1-scanline row offset" claim survived as long as it did. Don't trust visual
  inspection — differences of 2-4% are invisible to the eye but indicate timing bugs.
- **PNG file size is not a signal.** NESER and Mesen2 use different encoder settings, so
  byte-for-byte identical frames can be 65 KB and 1.1 KB. Compare decoded pixels, never
  file sizes.
- **CRC-based integration tests**: Capture frame CRCs at known stable points (e.g., frame 600) and use as golden values for regression testing. Update test comments to reference GitHub issues for known differences.
- **Screenshot settings for comparable captures**:
  - Mesen2: `--Video.VideoFilter=None --Video.AspectRatio=NoStretching --snes.disableFrameSkipping=true`
    (the frame-skip switch is mandatory for animated content; see step 9 of the Instructions)
  - Since the BG vertical-scroll display-line fix (issue #2945, PR #2981), NESER and
    Mesen2 frame-N captures align **byte-for-byte at zero row offset**. A previously
    documented "constant 1-scanline row offset vs NESER" was in fact a NESER BG bug,
    not a capture convention. Still run a ±1-row shift search when diffing, but treat
    any nonzero best shift as evidence of a bug, never as a convention to allow for.
- **Always diff before documenting**: Before creating a "known issue" for a visual difference, run a pixel diff to quantify the discrepancy and locate where differences occur (edges, specific scanlines, etc.).
- **Regenerating many goldens after a systematic rendering fix**: when a fix invalidates
  a large set of screen-CRC goldens (e.g. a whole-BG shift), don't re-approve blindly and
  don't re-approve one-by-one either. Build pre-fix `main` in a `git worktree` (note:
  worktrees do not materialize the `snes_test_roms` submodule — symlink it from the main
  checkout), run the suite there with `NESER_CAPTURE_SCREEN=1` to regenerate the
  previously-approved captures, then programmatically verify each post-fix capture is the
  *exact expected transform* of its approved predecessor (for #2945: `new[y] == old[y+1]`
  for every pixel, for every static-screen golden). Prior human approvals then carry over
  to the new CRCs. Cross-check samples from every affected suite against fresh Mesen2
  captures as well; ROMs with scanline-anchored effects (HDMA banding) won't satisfy a
  pure shift and must be verified against Mesen2 directly.
- **Reading the diff image**: `--out` marks differences red over a dimmed greyscale base, which
  immediately separates localized issues (edge effects) from systematic ones (whole-frame shifts).

### Approving new screen-CRC baselines (settle-probe workflow, from #2878)

Established for the PPU BG suites and intended for reuse by the remaining
epic-#2724 visual suites (#2879, #2880, #2881, #2883, #2884):

1. **Settle-probe each ROM** headlessly (temporary `#[ignore]` test using
   `Snes` directly): record the screen CRC after every frame up to ~1800.
   The settle frame is the start of the final run of identical CRCs. If the
   run is >= 600 frames long the ROM is *static*: sample at settle + 60. If
   the CRC never stabilizes it is *animated*: sample at a fixed frame
   (e.g. 600) — but see step 4. Two probe-hygiene rules from #2881:
   encode `frame` and `crc` in every capture PNG filename (the run
   survives a lost log), and write the probe log to a file — a
   background command piped through `tail` keeps only the final lines.
   An early settle does NOT mean the ROM is static by design: PeterLemon
   demos poll ReadJOY every frame and only animate on input — read the
   ROM's input loop before concluding anything from a settle, then
   exercise the feature with scripted holds/taps (each held frame is one
   deterministic step in a per-frame poll loop, identical in NESER and
   Mesen2). ROMs with splash screens can swallow early taps entirely
   (hdrvtest ignores input until ~frame 300): schedule taps after the
   first screen's settle frame, and compare each scripted combo's CRC
   against the no-input CRC — equality means the taps were swallowed.
   **Always convert probe decimal CRC output to hex via Python**
   (`python3 -c "print(hex(N))"`) before comparing against Mesen2
   values — mental arithmetic on 10-digit decimals is error-prone and
   produces silent false mismatches (caught in #3021: `4015300223` was
   misread as `0xEFB6E03F` instead of the correct `0xEF549E7F`).
2. **Capture and pixel-diff**: write NESER PNGs during the probe, capture
   Mesen2 headless at the same frame (`--testRunner` + Lua `print()` hex
   dump; flags above), then diff with `python -m scripts.diff_screenshots
   <neser> <mesen> --shift-search 1`. Never approve by eye. Where the upstream project ships reference
   screenshots (e.g. PeterLemon), diff those too.
2b. **Verify the golden's power by mutation before committing it** (from #3083).
   A golden that matches is only worth having if it would stop matching. Break
   the behaviour it is supposed to pin and watch it fail; if it doesn't, you have
   recorded a screenshot, not an oracle. State in the comment what the mutation
   showed, because it is also the honest boundary of what the vector covers.
   For the undisbeliever HDMA goldens: making `hdma_transfer_due` skip scanline 0
   (a one-line banding phase shift) breaks all three, while moving
   `HDMA_TRANSFER_POSITION` from dot 276 to 277 breaks none — the write still
   lands in hblank ahead of the next line's dot-22 INIDISP latch. So they are a
   frame-phase oracle and not a within-line clock oracle, and nobody should later
   read a green run as evidence about sub-line timing.
3. **Only exact matches become goldens.** Divergent ROMs stay vendored but
   un-automated, with one bug issue per distinct root cause carrying the
   pixel-diff evidence (policy in docs/SNES_TEST_ASSET_POLICY.md).
4. **Animated ROMs need a phase check, not just a content check**: search
   wrap-around shifts (`ImageChops.offset`) and neighbor frames to separate
   "same animation, different phase" from real rendering differences. Since
   #2990 (which fixed a swallowed-vblank frame counter and the RDNMI
   read-hold window) NESER matches Mesen2 pixel-exactly at equal frame
   numbers for NMI- and RDNMI-poll-driven animation. The animated-golden
   workflow: derive CRCs directly from frame-skip-free Mesen2 captures
   (decode the PNG to the `screen_snapshot` RGB layout, `zlib.crc32`), pin
   scrolling/cadence-sensitive ROMs at several spread-out frames (e.g.
   120/360/600) so a phase regression cannot slip past one lucky sample,
   and additionally pixel-diff contiguous windows (e.g. 118-122, 598-601).
   Cross-frame offset matching (find k where NESER f(N+k) == Mesen2 f(N)
   exactly) separates constant init offsets from accumulating drift — a
   constant k at two distant windows means an init-time difference, a
   growing k means a cadence bug. Do not bake a phase offset into a golden.
5. **Exploit twin ROMs for triage**: when a source provides with/without
   feature pairs (e.g. vmain `no-remapping` vs `with-remapping`), a clean
   pass on one twin and a large diff on the other pinpoints the defective
   feature immediately (that's how #2989, VMAIN $2115 bits 2-3 ignored, was
   isolated in minutes).
6. **Building undisbeliever ROMs on macOS**: bass-untech miscompiles under
   clang (unspecified argument-evaluation order dangles a `nall::vector`
   reference, breaking bare `define NAME` and failing every assembly with
   "Rom block code does not exist"). Apply the patch documented in
   `roms/snes/automated_tests/snes_test_roms/undisbeliever-ppu-bg/README.md`
   before building; it does not change assembled output. The patch is
   never committed (the bass-untech submodule stays pristine), so a
   previously *built* `bass/out/bass-untech` binary may still be
   unpatched: on any "Rom block ... does not exist" failure, re-apply
   the patch, `touch` a bass source file and rebuild before debugging
   anything else (seen again in #2879 after a fresh submodule checkout).
7. **When authoring OBJ test ROMs, park unused OAM entries at X=256,
   not y=240** (from #2879): the conventional y=240 filler is not
   off-screen for 32px-tall sprite sizes (OBSEL 5/6/7 smalls, any
   large) -- OAM Y is 8-bit, so the sprite wraps into screen lines
   0-15, and even at X=256 an in-range sprite still consumes the
   per-scanline range/time limits (the X=256 bug). 125 wrapped fillers
   starved the visible sprites' tile slivers in Mesen2 and masked the
   scene under test; parking at X=256 with a Y clear of any visible
   line is safe at every size (this interaction is also how #3003 was
   found).
7b. **Spec-check every cartridge-header field you hand-write in a
   synthetic ROM, including the ones nothing seems to read** (from
   #3137). Header bytes get copied between test ROMs and drift from the
   image they describe, and an emulator that ignores a field today may
   use it tomorrow -- at which point the fixture is quietly wrong and
   the failure looks like an emulator bug. `minimal_snes_rom` declared
   ROM size `$07` on a 64 KB buffer; fullsnes ("Cartridge Header") gives
   `$FFD7` as `(1 SHL n)` KB, so `$07` claims 128 KB and the correct
   value is `$06`. Note the trap in the fix: a reviewer offered "reword
   the comment or correct the value" and the comment was *accurate* about
   what `$07` means -- rewording would have preserved a header that lies
   about the ROM. Check the field against fullsnes and correct the
   value. Rounding-up in that entry applies only to non-power-of-two
   carts, so an exact power of two has an exact `n`.
8. **Verify upstream test-ROM naming against official spec terminology
   before documenting it.** Test-ROM sources can invert or localize the
   official names: undisbeliever's `object-dropout-test.asm` calls the
   32-OBJ/line limit `TimeOverflowTest` and the 34-sliver/line limit
   `RangeOverflowTest`, which is exactly backwards from the official
   $213E flag names (bit 6 = range over = >32 OBJs; bit 7 = time over =
   >34 slivers). Write issues/READMEs in official fullsnes terminology
   and note the upstream discrepancy explicitly (caught late in #2879's
   asset README and bug issue #2999).
9. **OBJ eval/fetch pipeline specifics (from #2999, verified in both
   Mesen2 `SnesPpu.cpp` and ares `object.cpp`)**: sprites shown on line
   N are evaluated during line N-1 (H=0..255, one OAM entry per 2 dots)
   and their tile slivers fetched during H=270..339 of line N-1 as 35
   two-dot attribute-fetch slots of which only 34 get CHR data — the
   35th attempted fetch raises time over. Fetching walks the in-range
   list in REVERSE evaluation order, so on overflow the FIRST/front-most
   sprites lose their slivers (the `_Flipped` ROM variant distinguishes
   this). The range check includes horizontal visibility (fully
   off-screen-left sprites are not in range) except raw X=256, which
   counts for range AND consumes its full width of time budget without
   drawing (Mesen2 `SpriteInfo::IsVisible`/`endTileX`; ares
   `onScanline`/`x != 256` column skip). Two reference DISAGREEMENTS to
   ask the navigator about, not guess: forced blank during the eval
   window (Mesen2 pauses the OAM cursor — entries deferred, plus a
   stale-latch one-entry drop on resume; ares skips entries
   permanently — NESER uses the pause model without the stale-latch
   drop), and the time-over flag dot (Mesen2/ares raise it inside the
   fetch window; fullsnes says H=0 of the displayed line — NESER
   follows Mesen2/ares per navigator decision). OBJ interlace (SETINI
   $2133 bit 1, from #3000): gated on bit 1 ALONE (no screen-interlace
   dependency); the in-range test keeps the OAM Y anchor with height
   halved (`height >> 1`), and the fetch doubles the line-within-sprite
   and ORs in the frame field BEFORE V-flip mirrors the doubled
   coordinate against the FULL height (Mesen2 `IsVisible`/
   `FetchSpriteAttributes`, ares `onScanline`/fetch — identical
   arithmetic incl. the rectangular split-mirror). Beware plausible
   wrong models that coincide at Y=0 (doubling the compare line anchors
   sprites at half their OAM Y): test geometry at Y != 0.
10. **When authoring mode 5/6 (hires) scenes, define char N+1 for every
   used char N** (from #2881): BG1/BG2 tiles are 16 px wide in these
   modes (fixed 16x8), so each map entry fetches chars N AND N+1 — a
   scene defining only chars 0-8 gets transparent right tile halves
   wherever char 8 is used (char 9 is empty VRAM). Exploit deliberately
   or avoid, but decide (NESER implements the pairing since #3016,
   closing #3019).
11. **Building WLA-DX assets with the modern toolchain** (from #2881):
   Homebrew wla-dx is v10.x; upstream wla.bat recipes from v9.5 need
   `wla-65816 -o out.obj in.asm` and wlalink flags split (`-v -r`, not
   `-vr`). Document the toolchain version in the asset README.
12. **`git subtree add/pull` requires a fully clean tree** — including
   modified content inside nested submodules: revert the transient
   bass-untech clang patch (`git -C .../bass-untech checkout -- .`)
   before any subtree operation, re-apply to build (hit twice in #2881).

### A passing screen-CRC test proves the screen is UNCHANGED, not that the ROM passes (from #3092)

`RunOracle::ScreenCrc` sets `passed = (actual_crc == expected_crc)` and has
no notion of a self-checking ROM's own PASS/FAIL backdrop. So a green
screen-CRC test means only "the screen still hashes to the recorded value" —
which may be a recorded *failure*. Never read one as evidence the ROM passes,
and never conclude a fix worked because such a test is green.

**Two conventions exist for a divergence record; pick deliberately and say
which in the reason string.**

- *Aspirational* — record the Mesen2-correct PASS CRC, keep `#[ignore]`. The
  test FAILs under `--include-ignored` today and turns green exactly when the
  bug is fixed. **Prefer this** for anything you intend to fix.
- *Record-current* — record NESER's own diverging CRC. Passes under
  `--include-ignored`, meaning "still diverging as recorded". This **inverts
  the fix signal**: a correct fix turns the test red and reads as a
  regression. Only justifiable when there is no single correct value to
  assert (e.g. a pixel-diff comparison against a picture, not a PASS/FAIL
  backdrop).

Consequences worth knowing before touching such a suite:

- A test named `*_passes` asserting a record-current golden is a lie in
  waiting. Under the aspirational convention the name and the usual
  "expected screen-CRC PASS (blue)" assert message become true for free, so
  conversion needs no renaming.
- `#[ignore]`d goldens rot silently. #3092 found one asserting a CRC that
  belonged to a **different ROM**, undetected from #2883 onward, and two more
  invalidated by a capture-geometry change (#3001) nobody re-ran. Audit
  ignored goldens whenever a rendering or capture convention changes.

### Read the ROM's pass()/fail() epilogue before blaming the subsystem under test (from #3116)

A self-check ROM's screen is the output of two independent things: the
behaviour being tested, **and** the ROM's own verdict-reporting path. If the
verdict path is broken, the screen tells you nothing about the behaviour — but
it looks exactly like a subsystem bug, and it will absorb unlimited
investigation.

Every ROM in byuu's "SNES TEST IMAGE" family (KungFuFurby, jonasquinn mirrors:
`test_hdma*`, `test_irq*`, `test_nmi`, `test_dma*`, `test_vram`) ends:

```asm
pass() { ... lda #$7c : sta $2122 ... lda #$0f : sta $2100 : stp }   ; blue $7c00
fail() { ... lda #$1f : sta $2122 ... lda #$0f : sta $2100 : stp }   ; red  $001f
```

`fail()` is assembled **directly after** `pass()`. NESER implemented `STP` as a
no-op, so a PASS run painted blue, fell through the dead halt into `fail()` ten
CPU cycles later — same scanline, so no frame ever showed the blue — and then
executed the following bytes as code. Sixteen vendored binaries were affected.
Three issues (#3062, #3063, #3093) had recorded "FAIL" screens that were
post-halt garbage; two of them had already burned root-cause hunts on
subsystems that were not defective, and one issue had concluded a specific
mid-frame HDMA-disable bug purely from the colour.

Habits that catch this class:

- **Read the epilogue first.** The sources are vendored under
  `roms/snes/automated_tests/snes_test_roms/jonasquinn-test-roms/*/`. Confirm
  they match the binary you run (md5 the mirrors, or search the ROM for a
  distinctive data table) — several ROMs ship in more than one build.
- **A wrong FAIL shade is a red flag, not a diagnostic.** #3062 built a
  taxonomy of red levels (r=31/14/10/8) and hypothesised the shade encoded
  *which* check failed. Every one was post-halt garbage. If the observed colour
  is not one the ROM's source can actually write, the ROM is not the thing
  reporting it.
- **Check the halt is modelled** before trusting any screen from a ROM that
  ends in `stp` (or a `bra *` self-loop). A CPU that runs past its own halt
  invalidates the entire suite at once.
- **Prefer the ROM's machine-readable results** where they exist. These ROMs
  declare 32 KB SRAM and write verdicts to bank `$70`, so the emitted `.sav`
  names the failing sub-test with no tracing: `test_hdma.smc` byte 0 is a
  sub-test number (0 = pass), `test_hdmatiming.smc` `$00..$3F` compares against
  an in-ROM `compdata` table, `test_hdmasync.smc` `$000..$7FF` against
  `test_hdmasync_data.bin`.

### Check the sample frame can actually SEE the divergence (from #3092)

Before trusting any screen-CRC divergence record, capture **both** emulators
at the test's own sample frame. If NESER and Mesen2 agree there, the test is
asserting agreement with the reference, not a divergence — it cannot fail no
matter how the feature under test behaves.

`test_hdmasync.smc` sampled frame 600 for two years. Both emulators render
flat black until frame ~1027; Mesen2 reaches blue PASS at 1029 while NESER
paints red. The frame-600 assertion was recording a shared black screen, so
it would have passed under any HDMA-sync behaviour whatsoever.

Run a settle probe out to several times the sample frame before believing a
"renders black / hangs" description — a slow ROM and a hung one look
identical at a frame sampled too early. Binary-search the reference's
transition frame, then sample well past it with margin on both sides.

### Identify a mystery flat-fill CRC by enumerating BGR555 (from #3092)

When a golden's literal means nothing to you, test whether it is a uniform
fill before assuming it is structured content. SNES 5-bit channels expand as
`(v << 3) | (v >> 2)`, so all 32768 representable colours can be brute-forced
against a target CRC in seconds:

```python
exp = lambda v: (v << 3) | (v >> 2)
for r in range(32):
    for g in range(32):
        for b in range(32):
            px = bytes((exp(r), exp(g), exp(b)))
            if zlib.crc32(px * (256 * 224)) == target:
                print(r, g, b)
```

Scan the full space, not a handful of guessed colours: a five-colour probe in
#3092 wrongly concluded `0x8662_6F50` was "not a flat fill" when it is a flat
`(66,0,0)`. Identical CRCs across unrelated ROMs are expected and not
evidence of copy-paste — a flat fill of one colour and size hashes the same
whichever ROM produced it.

Useful corollary: this ROM family encodes FAIL detail in the backdrop's red
level (r=31, 14, 10, 8 seen), so the shade may say *which* sub-check failed —
a free diagnostic worth confirming against the ROM source before tracing.

### Mesen2 capture-dimension conventions (from #2879, extended #2881; settled by #3001/#3034)

**Current state: no normalization is needed.** NESER's display-mode
output now matches Mesen2's raw framebuffer geometry, so diff captures
directly and expect 0 differing pixels at the native size. A nonzero
diff is a bug, not a convention mismatch.

- **Any hires or interlaced frame** is 512x448 in both. #3001 aligned
  the width and the overscan window; #3034 made the layout a single
  frame-sticky latch and adopted Mesen2's row doubling, so a
  progressive hires frame fills both rows of each display line instead
  of NESER's earlier 512x224.
- **239-line overscan** ($2133 bit 2) is the standard 224-line window
  in both (NESER renders all 239 lines internally and clips rows
  7-231; #2879 measured the exact offset, #3001 applied it).

The history matters when reading older test-file comments, which still
describe the pre-#3001/#3034 world: NESER used to render 256x448 for
interlace and 512x224 for progressive hires, so goldens approved back
then were verified only *after* halving Mesen2's width or height. Those
goldens were re-approved at the native geometry in #3034. If a comment
tells you to halve a Mesen2 capture, it is stale -- check the code.

- **Structure sampling attributes hires/interlace divergences fast**
  (from #2881): before deep analysis, sample whether each capture is
  column-doubled and/or row-doubled (compare px[x,y] vs px[x+1,y] /
  px[x,y+1] on a grid). NESER-vs-Mesen2 structure disagreement pinpoints
  the broken axis immediately, and upstream reference screenshots can
  arbitrate which structure is correct (that's how #3016 hires columns
  and #3017 line-doubled interlace fields were separated in minutes).
- **Sample the reference capture's structure to confirm a model, not
  just to localise a bug** (from #3034). Before adopting Mesen2's row
  doubling, asking "are all of Mesen2's even/odd row pairs identical?"
  turned a claim read out of `ApplyHiResMode` into a measured fact
  about its output -- and the same query on a mid-frame-switch capture
  showed rows 0-199 column-doubled and 200-447 in half-pixel pairs,
  which is the whole retroactive-conversion model visible in one
  measurement. Source reading tells you what to look for; the capture
  tells you whether it is true.
- Where a genuine geometry divergence does appear, the old policy still
  applies: commit the vector `#[ignore]`d with NESER's current CRC
  recorded, and say in the comment what was content-verified and what
  was not.

### Replaying scripted input in Mesen2 (interactive-ROM baselining, from #2879)

To baseline interactive ROMs (menus needing joypad input), drive NESER
with `rom_runner`'s frame-stamped `InputEvent` scripting and replay the
identical schedule in Mesen2:

1. Have the Rust probe print each combo's schedule
   (`frame:Button:pressed` list) and sample frame; generate one Lua
   script per combo from that output so both emulators consume one
   source of truth.
2. In Lua, count frames via the `startFrame` event and apply every edge
   with `stamp <= frameCount - 1` to a persistent `state` table; call
   `emu.setInput(state, 0)` from an `emu.eventType.inputPolled`
   callback (setting state only in `startFrame` can be overwritten by
   Mesen's own input update). SNES button names: `a b x y l r start
   select up down left right`.
3. Same-numbered frames align between the two emulators (validated
   byte-for-byte in #2878/#2879), so capture at the NESER sample frame.
   **Validate the replay pipeline with a control golden first** (from
   #3000): before approving any NEW golden, replay one already-approved
   combo of the same suite through the exact same Lua/capture/diff
   pipeline and require it to reproduce its approved capture at 0
   differing pixels. A passing control proves scripts, flags, frame
   alignment and diff code end-to-end; only then do 0-pixel results on
   the new combos count as approvals.
4. Menus that poll button *level* once per frame need taps held exactly
   1 frame; space taps generously (test_oam needed an 8-frame period --
   4 dropped presses during menu redraws). The dialed values are
   usually visible on screen, so captures self-verify the cadence.
5. **Locking auto-advancing demos**: some demos free-run until *any*
   button is pressed but navigate on only a subset of buttons -- read
   the ROM's input handler to find a button the navigation loop
   ignores. undisbeliever's `window-shapes-single.sfc` exits its timer
   loop on any bit of the 16-bit `joypadPressed` word but its
   navigation loop reads only the JOYH byte (B/Y/Select/Start/dpad), so
   a scripted **A tap** (JOYL byte) freezes the auto-advance without
   changing the selection. Schedule the lock tap twice (e.g. frames 40
   and 48) so a press landing before auto-joypad is live cannot be
   missed (#2880).

### Probing animated ROMs with a per-frame CRC sweep (from #2880)

Before choosing screen-CRC sample frames for animated or
frame-scheduled content (fades, brightness steppers, image cycles), map
the plateaus empirically with a throwaway probe test instead of
trusting the nominal schedule: load the ROM like `rom_runner` does
(`Snes::new` + `load_rom`), tick, and on each `is_ready_to_render()`
frame print `snes.screen_crc32()` **only when it changes**. One run
gives the full plateau map (e.g. #2880's fade demo: 4-frame brightness
steps from frame 12, holds at 68-135 and 257-324, black gap 192-200,
378-frame cycle; the brightness stepper's level-N plateau spans frames
64N+8 through 64N+71). Then pick mid-plateau sample frames with >=2
frames of margin on each side and delete the probe before committing
(it needs `crate::platform::emulator::Emulator` in scope for
`load_rom`). This replaces dozens of single-frame runs and guards
against off-by-a-few boot-frame offsets between the ROM's internal
counter and the emulator frame count.

### Attributing divergences by exact diff-pixel counts (from #2880)

When a Mesen2 cross-check diffs, compute the pixel area of the region
each suspect rule governs and compare against the reported diff count
before writing the bug issue -- an exact match pins the divergence to
one rule and makes the issue evidence unambiguous. In #2880 the
half-math scenes diffed in exactly 7424 px = the 8192 px sub-transparent
fallback region minus 768 px of black bars that halve to themselves,
and the center-only variant diffed in exactly 38912 px = its enlarged
fallback area -- proving NESER fails to suppress halving for the
fixed-colour fallback (#3012) while all 64 sub-opaque crossings match.
Design NESER-authored scenes to make this possible: give each rule its
own screen region (quadrant layouts) and include degenerate values
(black bars, white bars) whose invariance under the suspect operation
is predictable.

### Self-verifying goldens for vectors no reference can arbitrate (from #3003)

When the references disagree, a screen-CRC golden rests entirely on your reasoning being
right, and there is no capture to diff against. Look for a way to derive the expectation
from the ROM's **own output** instead -- an internal relation that must hold whatever the
correct absolute answer is.

`obj-y-wrap.sfc` is the worked example. It renders the same 16x32 sprite twice at the same
Y, differing only in the V-flip bit, and the visible wrapped window is exactly the sprite's
lower square half. So unflipped screen row `L` shows source line `16 + L`, the flipped one
shows `(16 + L) ^ 15 = 16 + (15 - L)`, and therefore `flipped(L) == unflipped(15 - L)` --
which IS the "rectangular OBJs flip as two stacked squares" claim, expressed without any
absolute screen row, without the OAM-Y-to-framebuffer-row convention, and without a
reference emulator. It fails on 8 of 16 rows before the fix and 0 of 16 after.

Two things make this stronger than a cross-emulator diff:

- It cannot be wrong about the reference, because it does not consult one.
- It is **scale-invariant**, so it can be evaluated on a GUI emulator's own screenshot even
  when that emulator has no headless mode and saves at a scaled size. Checking the relation
  on ares' screenshot (equal lit-pixel counts in the two bands, 196 vs 196) while it fails
  on Mesen2's (98 vs 90) settled #3003 empirically without any pixel alignment work.

Prefer relations over absolute expectations whenever a ROM renders the same content twice
under different register settings -- design NESER-authored ROMs to make that possible.

### Re-measure any issue that cites a number before planning a fix (from #3083)

A pixel-diff percentage in an issue body is a measurement with a timestamp, not a
property of the code. #3083 reported "~55-60% per-pixel difference" across 13 ROMs
and asked for a frame-phase investigation; re-measured, every one of them was a
0-pixel match. The issue was filed 2026-08-01 and 0df212be (`$43xA` HDMA line
counter, #3062) merged two days later. The whole planned investigation was for a
bug that no longer existed.

Two checks, both cheap, before designing anything:

```bash
gh issue view <n> --json createdAt
git log --oneline --since=<that date> -- src/snes    # what landed after it was filed
```

and read the close dates of every issue in the body's own "Related:" list — #3083
named five, four of which had closed *after* it was written. A related issue closing
later is the strongest single hint that the premise has moved.

Stale premises are not limited to the headline number. #3083 also stated the affected
ROMs "have no committed screen-CRC goldens yet"; 10 of the 13 had been automated in
#2981. Verify each factual claim in the body against the tree, not just the one the
title is about.

When the answer is "already fixed", say so with the measurement table and close it —
and be careful not to let a well-argued hypothesis survive its own disproof. #3083's
leading suspect (HDMA-during-GPDMA never being armed, `dma_tick` in `system_bus.rs`)
is a real unmodelled gap and was verified in source, but it was not what those ROMs
were showing. It went to its own issue (#3139) instead of justifying a speculative
restructure.

### Measure the pre-change baseline BEFORE touching golden-shifting code (from #3021)

Any change to shared timing (DMA cost models, CPU cycle counts, PPU trigger
positions) will move several screen-CRC goldens at once, and a raw "N pixels
differ vs Mesen2" number after the change is **uninterpretable on its own** --
it cannot distinguish an improvement from a regression. Before implementing,
capture the current NESER frames and record each vector's pixel diff against a
fresh Mesen2 capture. If you only realise this afterwards, recover the baseline
with `git stash push -u` -> capture -> `git stash pop` rather than guessing;
in #3021 that recovery is what turned an apparent "the HDMA envelope regressed
StarWars" into the actual result (2 vectors improved, 1 regressed, net
positive), and it changed the recommendation given to the navigator.

Tabulate every affected vector before/after. A mixed result is a legitimate
outcome to report -- what is not legitimate is reporting only the vectors that
moved in the direction you expected.

### Source-tag and clock-gate trace hooks (from #3021)

Two cheap additions make register traces far more diagnostic:

- **Tag the writer.** Emit `src=C` for CPU stores and `src=D` for DMA/HDMA
  B-bus writes. An ordinal-aligned diff then shows immediately when one
  emulator performs an HDMA burst the other skips -- invisible if both look
  like anonymous `$21xx` writes.
- **Gate on a clock range** (`NESER_TRACE_FROM` / `NESER_TRACE_TO` env vars).
  Tracing every write for 365 frames produces 100k+ lines; gating to the
  window of interest keeps both logs comparable and fast.

Both are implemented as of #3050 -- this is no longer a recipe to build from
scratch:

- **NESER**: `--trace-cpu=2` (or `NESER_TRACE_CPU=2` for the headless
  integration runner) emits one clock-stamped line per CPU bus cycle --
  `read`/`write`/`internal` with `clk=<master clock>`. `--trace-ppu=3` /
  `NESER_TRACE_PPU=3` adds `clk=`/`lc=` to every register access.
  `--trace-from=`/`--trace-to=` (or `NESER_TRACE_FROM`/`NESER_TRACE_TO`)
  narrow every clock-stamped line to a master-clock window. Any ROM suite can
  be traced headless:

  ```bash
  NESER_TRACE_CPU=2 NESER_TRACE_FROM=53605200 NESER_TRACE_TO=54900000 \
    cargo test --no-default-features --lib starwars_f600 -- --ignored --nocapture
  ```

- **Mesen2**: `Core/SNES/NeserBusLog.h` (a local, non-upstream hook) logs
  `mesen busR/busW/idle <addr> ticks=<clk>` to stderr under `NESER_BUS_LOG=1`,
  gated by the same two env var names. It is called from
  `SnesMemoryManager::Read`/`Write` (which sample `_masterClock` at the instant
  the data moves) and from `SnesCpu::Idle` / the native-mode branch of
  `IdleOrDummyWrite`. Logging idles matters: an ordering divergence shows up as
  `idle` on one side against `busR` on the other, which a read/write-only log
  cannot see.

- **Diff**: `python -m scripts.diff_bus_traces <neser.log> <mesen.log>`.

Two traps when tracing DMA specifically, both hit in issue 3042:

- **Mesen2's DMA traffic does not go through `Read`/`Write`.** `CopyDmaByte` uses
  `_memoryManager->ReadDma`/`WriteDma`, which the original hook did not cover, so a
  window spanning a transfer logged *nothing but `idle`* -- easy to misread as "the CPU
  is halted and there is nothing to see" when in fact the entire burst was invisible.
  `NeserBusLog::LogDma` now covers both; it also logs the value, because clocks alone
  cannot tell two adjacent VMDATA bytes apart when comparing against NESER's
  `write 2118=03` PPU trace.
- **Rebuilding `make core` is not enough.** The app extracts its embedded copy of
  `MesenCore.dylib` into `~/Library/Application Support/Mesen2/` on every start and
  *overwrites* whatever is there, so a hand-copied dylib is silently reverted. Run
  `make ui` (which re-embeds the freshly built core) and use the binary under
  `bin/osx-arm64/Release/osx-arm64/publish/`. To confirm which copy is live:
  `DYLD_PRINT_LIBRARIES=1 <mesen> ... 2>&1 | grep -i mesencore`, and check the new code
  is really in it with `strings <dylib> | grep '<your new format string>'`.

Anchors line up better than expected: over 214 million master clocks both emulators were
within a few hundred of each other on the same ROM, so a window derived from NESER's
clocks can be reused verbatim on the Mesen2 side.

Stamp both sides at the *same point within the cycle* or every line is offset
by the access speed. NESER's reads are stamped `speed - 4` clocks into the
cycle and writes at its end, matching Mesen2's `_execRead`/`_execWrite` split.

### An exact bus-clock match does not imply matching pixels (from 3042)

A trace diff answers "do both emulators put the same values on the bus at the same
clocks". It says nothing about when each one *consumes* them. Issue 3042 was filed
against HDMA B-bus write-clock jitter and survived three rounds of DMA work; the trace
finally showed all 24 writes of the relevant burst identical in register, value **and**
master clock, while the frame still differed by 16 px. The cause was downstream: NESER
samples VRAM per dot inside `bg_pixel`, whereas Mesen2 pre-fetches tilemap and CHR into
`_layerData` in chunks bounded by H=263 and skips fetching while forced blank is set. On
a row the beam is drawing while a burst rewrites it, those two models disagree.

So when a pixel divergence outlives an exact bus match, stop looking at the producer and
go and read the consumer. The corollary is worth stating too: a residual's *name* can be
wrong for a long time. Three separate re-approvals attributed this one to write-clock
jitter because it moved whenever DMA or CPU timing changed -- moving with a change proves
sensitivity, not causation.

### Distinguish a transient divergence from a persistent one (from #3050)

The first ordinal whose clock offset leaves the baseline is often **not** the
bug. Intra-instruction ordering differences (an idle charged after an access
instead of before) show up as a `-4`/`+2` pair that returns to `0` on the very
next cycle -- real, but self-correcting. The one that matters is the last
ordinal at offset 0: everything after it has drifted for good. Compute both.

In #3050 the first divergence was an `abs,X` penalty-cycle ordering issue 5638
cycles *before* the actual cause, which was a 2-clock DMA end pad.

### The ProcessorTests vectors carry per-cycle bus data -- use it (from #3068)

`roms/snes/automated_tests/processor_tests/65816/` vectors record **every CPU cycle** as
`[address, value, signals]`, not just a cycle count. For years the harness asserted only
`cycles.len()` and the final CPU/RAM state, which is precisely why a whole family of
internal-cycle *placement* bugs (#3050, #3068) sat behind a green suite: moving an idle from
before an access to after it keeps the total identical and the final state identical.

`signals` is 8 characters, `vda vpa vpb rwb e m x mlb`:

- **opcode fetch** asserts both VDA and VPA (`dp-r-m--`); **operand fetch** VPA only
  (`-p-r-m--`); a **data access** VDA only (`d--r-m-l`). Filtering "data reads" on `vda`
  alone silently includes the opcode fetch and shifts every index by one.
- **Internal cycles** are `---r-m--` -- and their recorded `address` is *not* null, it is
  whatever was last left on the bus. The discriminator is the VDA/VPA flags, never a null
  address. (A synthetic `[null, null, ...]` sample in the harness's own unit test suggests
  otherwise; the real corpus does not look like that.)
- **`mlb`** (`l`) marks the locked read-modify-write bus cycles.
- The emulation-mode RMW **dummy write** is `---wemxl`: RWB=`w` with **VDA low**. Test RWB
  *before* the address-valid flags or it decodes as an internal cycle.

Worked example, `06.n` (ASL dp, `D=$5c7c`) -- the DL penalty and the RMW idle are both
visible in one vector:

```
[16765171, 6,   'dp-r-m--']  opcode fetch
[16765172, 130, '-p-r-m--']  operand fetch
[16765172, None,'---r-m--']  <- DL!=0 penalty idle, BEFORE the data read
[23806, 104,    'd--r-m-l']  data read
[23806, None,   '---r-m-l']  <- RMW idle, BETWEEN read and write
[23806, 208,    'd--w-m-l']  write
```

Practicalities: the committed CI subset is truncated to 32 vectors per opcode, so a
page-boundary edge case usually is **not** in it -- a green CI run does not mean the opcode is
clean. Fetch the full corpus for real verification, but fetch **only the opcodes you need**
(`raw.githubusercontent.com/SingleStepTests/ProcessorTests/main/65816/v1/<op>.<e|n>.json`,
~5 MB each); the whole corpus is ~2.7 GB. Note `write_subset` *wipes* the subset directory, so
the local corpus must also contain every currently-selected opcode or regeneration silently
drops them -- verify by checking the pre-existing files come back byte-identical.

### ProcessorTests targets a bare WDC 65816, not the 5A22 -- cputest.sfc wins for SNES (from #3068)

Instruction 4 lists ProcessorTests as a cross-check for CPU timing. It is not an unconditional
authority: the vectors describe a **bare WDC 65816**, while `gilyon/cputest.sfc` (1610 tests)
was validated on **real SNES hardware**, i.e. on the 5A22 core NESER actually emulates.

They genuinely conflict. In #3068, on emulation-mode direct-page indirect pointers:

| | `(dp,X)`, `DL != 0` | `PEI`, `DL == 0` |
|---|---|---|
| ProcessorTests | carries ($002DFF -> $002E00) | wraps ($000CFF -> $000C00) |
| `cputest.sfc`, Mesen2, NESER | wraps ($002D00) | carries ($000D00) |

Opposite directions, so no single rule satisfies both. Applying the ProcessorTests rule to
**either** opcode independently turned `cputest_passes_all_1610_tests` from its genuine
"Success" screen to Failed -- trading a 1610-test hardware oracle for 29 of 20 000 vectors.

So: **before adopting any rule change the vectors imply, run `gilyon_cpu_tests`.** If it
breaks, the vectors lose. Prefer carving the disagreement narrowly over disabling the whole
opcode. #3068 first carved by mode (cycle-exact checks dropped for all emulation-mode a1/d4
vectors); #3135 then enumerated the full corpus -- exactly 28 `a1 e` vectors plus `d4 e 232`
diverge, `a1.n`/`d4.n` are 0/10000 -- and replaced the mode carve-out with a named skip list
(`KNOWN_DIVERGENT_VECTORS`), so the other 19 971 emulation-mode vectors regained full
cycle-exact checking.

#3135 also settled authority: these vectors are simply **wrong**, not "right for a different
chip". The corpus is generated from an emulator model, not captured from hardware, and
upstream had already regenerated the long-indirect `[dp]`/`[dp],Y` vectors for exactly this
class of emulation-mode wrap bug (issue #1 / PR #2), with issues #3 (SBC `(dp,X)` -- the same
dispute as ours), #6 and #8 still open at the pinned revision. For PEI the WDC datasheet itself
documents the carry ("...and the PEI instruction which will increment from 0000FE or 0000FF
into the Stack area"), so even a bare 65816 disagrees with the vectors there; for `(dp,X)`
the in-page wrap is an undocumented quirk that gilyon's hardware-validated tests 02c9-02cc
pin down and Mesen2 ports as `GetDirectAddressIndirectWordWithPageWrap`.

Corollary for existing "cross-verified against cputest test NNNN" comments: such a comment
proves only that cputest exercises *one side* of a condition -- treat it as evidence about
the tested side, not the general rule. (An earlier revision of this corollary claimed cputest
"never tests `DL != 0`" for `addr_dp_x_ind`; that was itself wrong -- `make_cpu_tests.py`
tests 02cb/02cc exercise exactly that case in emulation mode. Check prose claims about what a
suite covers against the suite's generator source before repeating them.)

### Matching the reference's *rule* is not the same as matching its *quantity* (from #3067)

Porting a formula from Mesen2 is only valid if the value you feed it means the same thing in
both emulators. #3067 set out to adopt Mesen2's speed-aware `SyncEndDma` rounding for
general-purpose DMA -- the rule is right there in the source, and the identical change had
already made the two HDMA envelopes pixel-exact. Measured, it broke a Mesen2-approved
golden instead.

The reason was upstream of the formula: Mesen2 re-enters `ProcessPendingTransfers` from inside
`RunDma`, so an HDMA firing mid-transfer runs nested, pays no sync pads and folds its clocks
into the very counter the outer `SyncEndDma` rounds. NESER cannot nest, so its counter is a
different quantity. The rounding rule was correct and the input was not.

Two process lessons, both learned the hard way here.

First, screen CRCs alone produced a defensible-sounding but under-evidenced answer that review
correctly rejected. Second -- and this is the one that cost the most -- every absolute clock
number quoted in that investigation had to be retracted. One measurement attributed a delta to
the end pad that the mechanism could not produce; a later one used an undocumented anchor and
was not reproducible; and all of them were taken on a tree state that a concurrently-merged PR
then invalidated. What survived was the anchor-free statement: flip the literal, run the
vector, see which one diverges from its Mesen2 golden.

Prefer evidence of that shape. An absolute master-clock figure in a comment is a hostage to the
exact build, the exact anchor convention and the exact state of main; a "change X and this
named test fails" instruction stays true and is reproducible by the next reader in one command.

Before porting a formula, ask what feeds its inputs in each emulator, not just what the
formula says. If the surrounding structure differs, the local approximation you already have
may be the better model of the *net* behaviour, and "it matches the reference's source" is not
evidence that it matches the reference's behaviour.

### Scripted-input Mesen2 replay (testRunner) -- complete template

`--testRunner` uses a different Lua environment from `--loadScript`; this is the form that
works there, and it is what a scripted NESER vector must be compared against. `edges` takes the
numeric frame stamps straight from the Rust `hold(...)`/`InputEvent` list, and `target` is the
test's capture frame. Button names are Mesen2's (`"a"`, `"b"`, `"x"`, `"y"`, `"l"`, `"r"`,
`"up"`, `"down"`, `"left"`, `"right"`, `"start"`, `"select"`).

`--enableStdout` is required or `print()` reaches nothing. Note this section deliberately uses
`--testRunner`, unlike the file-I/O screenshot recipe further down, which needs `--loadScript`;
the two Lua environments differ and only the `print()`-based form works under `--testRunner`.

```lua
local target = 300
local edges = { {120, "r", true}, {151, "r", false} }
local state = {}
local frameCount = 0

emu.addEventCallback(function()
  frameCount = frameCount + 1
  if frameCount == target then
    local png = emu.takeScreenshot()
    local hex = {}
    for i = 1, #png do hex[i] = string.format("%02X", string.byte(png, i)) end
    print("SCREENSHOT_BEGIN")
    print(table.concat(hex))
    print("SCREENSHOT_END")
    emu.stop(0)
  end
end, emu.eventType.startFrame)

emu.addEventCallback(function()
  for _, e in ipairs(edges) do
    if e[1] <= frameCount - 1 then state[e[2]] = e[3] end
  end
  emu.setInput(state, 0)
end, emu.eventType.inputPolled)
```

Run it, then decode the hex between the markers to a PNG and pixel-diff:

```bash
until ! pgrep -f "Mesen --testRunner" >/dev/null 2>&1; do sleep 3; done
Mesen --testRunner <rom> <script.lua> \
  --Video.VideoFilter=None --Video.AspectRatio=NoStretching --snes.disableFrameSkipping=true
```

### Prove a fragile input-scripted vector with the SAME script on both sides (from #3067)

`peterlemon_ppu_advanced_tests::mosaic_mode5_sized` carries a doc comment saying its input
release margin was hand-tuned (150 -> 151) to win a latch race, which makes it tempting to
dismiss any movement as a tuning artifact and re-tune again. It is not dismissible: replaying
Mesen2 with the *identical* input script (`emu.setInput` edges matching the Rust
`hold(SnesButton::R, 120, 151)`) showed the approved golden at 0 px and the changed output at
12484 px. That turned "fragile test, probably fine" into decisive evidence against the change.

Whenever a scripted vector moves, script the reference the same way before concluding
anything. Re-tuning the margin to make the test pass again would have buried a real regression.

### A 0-px golden can depend on two errors cancelling (from #3050)

Fixing a real, Mesen2-verified timing bug turned undisbeliever's
`inidisp_forgot_to_force_blank.sfc` from pixel-exact to a fully black screen:
its previous perfection depended on the DMA error offsetting a separate CPU one
(the 65816 push/pull cycle ordering, since fixed in #3070). When a fix improves
several vectors and destroys one,
that is evidence about the *other* bug, not necessarily about the fix -- but do
not ship it. Narrow the change to the part the trace actually proves (there,
HDMA but not general-purpose DMA), pin the exclusion with a test, and say so.

Diff by **event ordinal**, not by clock, then histogram the per-ordinal clock
offsets. A single-valued histogram with zero transitions means clock-exact; a
histogram whose mode steps by a constant at specific ordinals localises the
divergence to those events.

### Mesen2 headless produces silent empty output when another instance runs (from #3021)

`Mesen --testRunner` can exit 0 having printed nothing at all -- no error, no
partial log -- if another Mesen process (often from a different worktree or an
earlier backgrounded run) is still alive. Two separate captures were lost to
this before the cause was found. Guard every capture:

```bash
until ! pgrep -f "Mesen --testRunner" >/dev/null 2>&1; do sleep 5; done
```

Treat an empty capture log as "check for a concurrent instance" first, not as
a bad Lua script -- the script is usually fine.

### Automating Screenshot Capture at Specific Frames

#### Mesen2 (Fully Automated)

Mesen2 supports Lua scripting to automate screenshot capture, but **file I/O is disabled by default** for security.

**Prerequisites:**
1. Enable file I/O access in Mesen2's settings:
   ```bash
   # Backup settings first
   cp ~/Library/Application\ Support/Mesen2/settings.json \
      ~/Library/Application\ Support/Mesen2/settings.json.backup
   
   # Enable I/O access (macOS/Linux)
   sed -i '' 's/"AllowIoOsAccess": false/"AllowIoOsAccess": true/' \
      ~/Library/Application\ Support/Mesen2/settings.json
   
   # Windows: use plain sed or edit manually
   ```

2. **Security note**: This allows Lua scripts filesystem access. Restore from backup when done:
   ```bash
   mv ~/Library/Application\ Support/Mesen2/settings.json.backup \
      ~/Library/Application\ Support/Mesen2/settings.json
   ```

**Lua Script Pattern:**
```lua
-- capture_frame.lua - Capture screenshot at specific frame
local targetFrame = 120  -- Frame number to capture
local frame = 0

function save(fname, data)
    local file = io.open(fname, "wb")
    if file then
        file:write(data)
        file:close()
        return true
    end
    return false
end

function onEndFrame()
    frame = frame + 1
    
    if frame == targetFrame then
        local pngData = emu.takeScreenshot()
        local scriptFolder = emu.getScriptDataFolder()
        local fname = scriptFolder .. "/frame" .. targetFrame .. ".png"
        
        if save(fname, pngData) then
            print("Screenshot saved: " .. fname)
        else
            print("ERROR: Could not save screenshot")
        end
    end
end

emu.addEventCallback(onEndFrame, emu.eventType.endFrame)
```

**Running the script:**
```bash
# Run Mesen2 with ROM and script (opens GUI, runs in background)
/Applications/Mesen.app/Contents/MacOS/Mesen \
  path/to/rom.sfc \
  --Video.VideoFilter=None \
  --Video.AspectRatio=NoStretching \
  --loadScript path/to/capture_frame.lua &

# Let it run for enough frames, then kill
MESEN_PID=$!
sleep 5  # Adjust based on how long to reach target frame
kill $MESEN_PID
```

**Output location:**
- Files save to `~/Library/Application Support/Mesen2/LuaScriptData/<script-basename>/`
- Example: `capture_frame.lua` saves to `.../LuaScriptData/capture_frame/frame120.png`

**Important notes:**
- `emu.getScriptDataFolder()` returns empty string (falsy) when `AllowIoOsAccess` is disabled
- Use regular mode with `--loadScript`, **not** `--testRunner` mode (different Lua environment)
- `emu.takeScreenshot()` returns PNG binary data as string
- `emu.stop(0)` does not work in regular mode; use external process control instead

(ares is not used for screenshot capture — it has no scripting support and is
a source-code reference only; see step 9 of the Instructions.)


