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
   - Cross-check against anomie's timing docs and Tom Harte / ProcessorTests 65816 vectors; treat ares/Mesen2 as implementation evidence only.

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
- **APU is asynchronous**: The SPC700 runs on its own ~1.024 MHz clock, independent of the main CPU. Port reads/writes are the only synchronization; many games rely on exact IPL upload timing.
- **VRAM access timing**: CPU access to VRAM/CGRAM/OAM is only safe during V/H-blank (or forced blank); the address-increment-on-read/write behavior of `$2116`–`$2119` is a frequent source of bugs.
- **INIDISP brightness change delay**: When INIDISP ($2100) is written mid-scanline to change brightness, the hardware has a delay (several pixels) before the change becomes visible on-screen. The exact delay timing differs between hardware revisions and is not fully documented in fullsnes. Emulators implement different delay models, causing ~4-6% pixel differences in test ROMs that hammer INIDISP mid-scanline (e.g., `inidisp_brightness_delay.sfc`). These differences concentrate at brightness transition edges and don't affect real games. See issue #2973.
- **BG vertical scroll is display-line based, off by one from the framebuffer row**: the BG tile fetch adds BGnVOFS to the raw display line (vcounter), and the first visible line is vcounter 1 — line 0 is never rendered — so framebuffer row `y` samples BG line `y + 1 + VOFS`. This is why games routinely write VOFS = -1 (0x3FF) to pixel-align a BG with the top of the screen. Confirmed in ares (`background.cpp` fetchNameTable: `voffset = vcounter() + vscroll`, early-return at vcounter 0) and Mesen2 (`SnesPpu.cpp`: `realY = _scanline`, renders scanlines 1..224). The same +1 applies to the offset-per-tile voffset paths and to Mode 7 (`realY = _scanline` there too). OBJ/sprites do NOT get the +1 in framebuffer-row space: evaluation happens one line early, so a sprite with OAM Y=k occupies framebuffer rows k..k+h-1 — a renderer indexed by 0-based output row needs +1 for BG/Mode 7 but not for OBJ. NESER got this wrong for tile BGs (while Mode 7 was correct) until issue #2945; the symptom was every BG one row too low, masked in Mesen2 comparisons as an apparent "1-row capture offset". See `effective_offsets`/`screen_y` in `src/snes/ppu/background.rs`.
- **Frame-to-frame CPU/PPU timing drift**: Emulators with imperfect CPU/PPU clock synchronization can exhibit systematic frame-to-frame timing drift where the same instruction executes at slightly different horizontal positions across frames (e.g., 2-clock shifts in a 3-frame cycle). This causes test ROMs with tight timing windows to show extra flickering lines or pixel-level visual differences from reference emulators. While low-priority for gameplay (real games have timing margins), it's detectable in test ROMs like `hdmaen_latch_test_2.sfc`. Root cause is usually cumulative rounding errors in the CPU↔PPU clock conversion or DRAM refresh position jitter. See issue #2971.
- **Copier headers**: `.smc` files may carry a 512-byte header; failing to detect/strip it offsets the entire ROM and breaks mapping detection.
- **I/O region speeds are Fast, not Slow**: B-Bus I/O (`$2000–$3FFF`) and CPU I/O (`$4200–$5FFF`) in system banks run at 3.58 MHz (6 master clocks), the same as FastROM. Only WRAM mirrors (`$0000–$1FFF`) and expansion (`$6000–$7FFF`) in system banks are slow (8 clocks). This is a common planning mistake when building cycle-accurate bus models.
- **H/V-IRQ CPU-dispatch pipeline delay**: the PPU's H/V-IRQ line (`$4211` TIMEUP bit 7) becoming true is *not* the same instant the CPU can act on it. bsnes' `CPU::irqPoll` (`sfc/cpu/irq.cpp`) samples the *previous* poll's stale line value before updating the new one, so a freshly-triggered IRQ only becomes a dispatchable "transition" (i.e. wakes `WAI` or fires the vector) on the *next* 4-clock (one-dot) poll — a fixed one-dot pipeline delay. TIMEUP register reads themselves stay instantaneous (`CPU::timeup()` reads the raw line directly); only WAI-wake/interrupt-dispatch timing has the delay. This detail is undocumented in fullsnes and was only found by reading bsnes source (`sfc/cpu/irq.cpp`, `io.cpp`, `sfc/cpu/timing.cpp`'s `stepOnce()`); see neser's `Ppu::poll_irq_dispatch`/`irq_edge_age` (landed for issue #2909/PR #2931) for a worked implementation.
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
- **absindx SA-1 conformance ROM automation traps**: the ROMs' `org $0000` result variables (`TestFinished`: 0=Running, 1=Passed, 255=Failed) are accessed by the SNES CPU via direct page with D=0, i.e. they live in **WRAM `$7E0000`** — polling the SA-1 I-RAM mirror at `$003000` instead reads a stale `$AA` left over from the I-RAM mirroring sub-tests. Even at the right address, a naive every-tick poll false-fails: `SA1RamProtectionTest` transiently `EOR #$FF`s its own result byte while probing I-RAM writability, and its SA-1 CPU parks in its post-test idle loop *before* the SNES finishes the trailing `TestBwRamSize` scan and writes the real result. `SA1VersionCodeTest` never reports Passed **even on real hardware** — disassembly of its release build shows `CheckResult` unconditionally taking the failed path (the `INC TestFinished` pass path at `$9EAB` is unreferenced dead code), deliberate since the true version-code value is unknown; treat its FAILED register-dump screen as the expected result. Its open-bus detection scheme is worth knowing: each register is read twice with different residual bus bytes (`LDA $2300,X` leaves operand `$23`; a `REP`-adjusted `LDA $AA,X` reaching the same register leaves `$AA`), so open-bus entries echo `23`/`AA` while real registers repeat their value. Both ROMs misbehave on Mesen2 (documented upstream and confirmed), so goldens must be navigator-approved captures of the emulator's own rendering.

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
- **Pixel-perfect comparison**: Use Python/PIL to compare screenshots pixel-by-pixel. Don't trust visual inspection — differences of 2-4% are invisible to the eye but indicate timing bugs.
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
- **Example diff script**: Use PIL to iterate pixels, mark differences as red, matches as dimmed grayscale — visual diffs immediately reveal whether issues are localized (edge effects) or systematic (whole-frame shifts).

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
2. **Capture and pixel-diff**: write NESER PNGs during the probe, capture
   Mesen2 headless at the same frame (`--testRunner` + Lua `print()` hex
   dump; flags above), then diff programmatically with a ±1-row shift
   search. Never approve by eye. Where the upstream project ships reference
   screenshots (e.g. PeterLemon), diff those too.
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
   or avoid, but decide; NESER missing this pairing is #3019.
11. **Building WLA-DX assets with the modern toolchain** (from #2881):
   Homebrew wla-dx is v10.x; upstream wla.bat recipes from v9.5 need
   `wla-65816 -o out.obj in.asm` and wlalink flags split (`-v -r`, not
   `-vr`). Document the toolchain version in the asset README.
12. **`git subtree add/pull` requires a fully clean tree** — including
   modified content inside nested submodules: revert the transient
   bass-untech clang patch (`git -C .../bass-untech checkout -- .`)
   before any subtree operation, re-apply to build (hit twice in #2881).

### Mesen2 capture-dimension conventions (from #2879, extended #2881)

Mesen2 screenshots are not always the PPU's native frame geometry; know
these before pixel-diffing display-mode captures:

- **Screen interlace** ($2133 bit 0, lo-res modes): Mesen2 emits 512x448
  by column-doubling; NESER renders 256x448. Halve Mesen2's width
  (`Image.resize((256, 448), NEAREST)`) before diffing.
- **Mode 5/6 hires and pseudo-hires, non-interlace**: Mesen2 emits
  512x448 by row-doubling (verified: all even/odd row pairs identical);
  NESER renders 512x224. Halve Mesen2's height before diffing. Mode 5/6
  WITH interlace is 512x448 native in both (directly comparable).
- **Structure sampling attributes hires/interlace divergences fast**
  (from #2881): before deep analysis, sample whether each capture is
  column-doubled and/or row-doubled (compare px[x,y] vs px[x+1,y] /
  px[x,y+1] on a grid). NESER-vs-Mesen2 structure disagreement pinpoints
  the broken axis immediately, and upstream reference screenshots can
  arbitrate which structure is correct (that's how #3016 hires columns
  and #3017 line-doubled interlace fields were separated in minutes).
- **239-line overscan** ($2133 bit 2): Mesen2 shows the standard
  224-line window; NESER renders all 239 lines. NESER rows 7-231 equal
  Mesen2's frame (search crop offsets; #2879 measured an exact match at
  offset 7).
- Because the raw framebuffers differ in geometry, such combos cannot
  carry Mesen2-approved screen-CRC goldens until #3001 settles a
  canonical convention -- commit them `#[ignore]`d with NESER's current
  CRC recorded, like the policy for real divergences.

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


