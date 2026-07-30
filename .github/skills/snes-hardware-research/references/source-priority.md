# SNES Hardware Research Source Priority

Use this order when researching SNES / Super Famicom hardware details:

1. **fullsnes (problemkaputt)**
   - Primary source: `https://problemkaputt.de/fullsnes.htm`
   - Start here for the memory map, CPU/PPU/APU registers, DMA/HDMA, timing notes, cartridge mapping, and enhancement-chip documentation.
   - Most comprehensive single-source SNES reference; treat as the primary authority.
   - **Anchor/pagination tip:** fetching a section anchor (e.g. `#snespictureprocessingunitppu`, `#snespputimersandstatus`) usually returns the top-level I/O map first, not the detailed register text. To reach the deep descriptions (register bit layouts, formulas, access timing), fetch the PPU/APU page with increasing `start_index` values — for the PPU section the detailed text lives roughly around offsets 16000, 28000, 40000, 56000, 72000, 82000. Page forward until you see the specific register name you need.
   - **Offline whole-page extraction (most reliable for deep sections):** download once with `curl -Lsf https://problemkaputt.de/fullsnes.htm -o /tmp/fullsnes.htm`, list section anchors with `grep -oiE 'NAME="[^"]+"' /tmp/fullsnes.htm` (e.g. input lives under `snescontrollersioportsautomaticreading`, `snescontrollersioportsmanualreading`, `snescontrollersjoypad`, `snescontrollershardwareidcodes`), then slice a section between consecutive `NAME="..."` anchors and strip tags with a short Python snippet (`re.sub(r'<[^>]+>','',chunk)` + `html.unescape`). This avoids guessing `start_index` offsets and returns the exact register/bit tables. Always verify guesses against the doc — e.g. `$4017` bits 2-4 are grounded and read `1`, not `0`.

2. **Direct retrieval with `curl`**
   - If the normal page fetch fails, retrieve fullsnes (or a mirror) directly with `curl -Lsf`.
   - Use this only as a transport fallback, not as a different source.

3. **anomie's SNES documents + SNESdev wiki**
   - anomie's docs: CPU (`anomie's 65816 doc`), PPU (regs/timing parts 1–2), DSP/APU, SPC700, memory map, timing.
   - SNESdev / Super Famicom wiki: `https://snes.nesdev.org/wiki/` and `https://wiki.superfamicom.org/`.
   - Use these for register bit meanings, timing diagrams, and behavior fullsnes summarizes only briefly.

4. **Vendored test-ROM hardware headers (fast and self-consistent)**
   - `roms/snes/automated_tests/snes_test_roms/undisbeliever-inidisp/sources/src/_common/registers.inc`
     documents every PPU/CPU register's bit layout with named constants, and it is the
     header the vendored undisbeliever *and* NESER-authored test ROMs actually assemble
     against -- so it is guaranteed consistent with what those ROMs do.
   - Reach for this FIRST for "which bit is which" questions. In #3011 it settled the
     W12SEL enable-vs-invert layout in one `grep`
     (`WSEL::win1 { enable = %0010, outside = %0001 }`) and the CGWSEL clip/prevent
     region encodings (`clip { never/outside/inside/always }`), which reading emulator
     source alone would have taken far longer to establish with the same confidence.
   - Caveat: its prose comments can contradict its constants -- the W12SEL comment says
     `i = Window 1 In/Out(1 = Inside, 0 = Outside)` while the constants say
     `inside = %0000, outside = %0001`. **Trust the constants**: they are what the ROMs
     assemble, and they are what agrees with ares/Mesen2.

5. **Test-ROM references for precise behavior**
   - 65816 CPU: Tom Harte / ProcessorTests `65816` vectors.
   - APU/SPC700 + S-DSP: blargg's SNES APU and SPC700 tests.
   - PPU: PeterLemon / krom SNES PPU test ROMs.
   - Treat passing/failing test vectors as authoritative for observable behavior.

6. **ares (preferred) and Mesen2 for implementation evidence**
   
   **Locating sources and binaries**:
   - **Sources**: Check for cloned repositories alongside the current repo first (e.g., `../ares`, `../Mesen2`)
   - **Binaries**: Check if built from source, then OS-specific standard locations:
     - macOS: `/Applications/ares.app`, `/Applications/Mesen2.app`
     - Linux: `~/Applications/`, `/usr/local/bin/`, `~/.local/bin/`
     - Windows: `C:\Program Files\`, `C:\Program Files (x86)\`
   - If not found, ask the user for the location before fetching from GitHub
   
   **ares** (Near/byuu's current emulator, successor to bsnes/higan):
   - GitHub: `https://github.com/ares-emulator/ares`
   - Useful entry points:
     - `ares/sfc/cpu/` and `ares/component/processor/wdc65816/` for 65816 behavior
     - `ares/sfc/ppu/` for PPU rendering and timing
     - `ares/sfc/smp/` (SPC700) and `ares/sfc/dsp/` for audio
     - `ares/sfc/cartridge/` for mapping and save hardware
     - `ares/sfc/controller/` for input devices
   - Represents Near/byuu's latest understanding of SNES hardware
   
   **Mesen2** (independent, highly accurate implementation):
   - GitHub: `https://github.com/SourMesen/Mesen2`
   - Useful source entry points:
     - `Core/SNES/SnesCpu.cpp` for CPU timing and behavior
     - `Core/SNES/SnesPpu.cpp` for PPU rendering
     - `Core/SNES/SnesMemoryManager.cpp` for bus timing, DRAM refresh, and memory access
     - `Core/SNES/Debugger/` for state inspection and tracing
   - Headless test mode: `Mesen --testRunner --enableStdout --timeout=N <rom> <script.lua>`
   - Screenshot settings: `--Video.VideoFilter=None --Video.AspectRatio=NoStretching`
   
   **Usage notes**:
   - Use only after checking fullsnes, anomie's docs, and the SNESdev wiki
   - Treat as implementation evidence, not as equal authority with written specifications
   - **When both ares and Mesen2 agree** on unspecified behavior, that's strong evidence
   - **Distinguish counter-evidence from absence of evidence** before deciding a split. Ask
     whether the second reference makes a CONTRARY claim about the *same quantity*, or simply
     has no model for it because it is structured differently. Those look alike and point
     opposite ways:
     - #3011 (CGWSEL clip mode 3 halving) and #3003 (OBJ V-flip mirror): ares computed the
       same quantity and disagreed → genuine counter-evidence, NESER followed ares.
     - #3035 (hires even-half sub coverage at x-1): ares composes hires colour math
       differently and has no cross-index lookup at all → absence of evidence, NESER
       followed Mesen2.
     Without this test the decisions read as arbitrary reference-shopping. Whichever way it
     lands, record the reasoning at the call site so the next reader can see why two nearby
     divergences resolved in opposite directions.
   - **Count LINEAGES, not implementations.** ares, ares-performance, higan and bsnes all
     descend from byuu/Near's work and routinely share an expression verbatim, so
     "three implementations agree" can be one opinion cited three times. Snes9x is the
     genuinely independent SNES lineage; an ares-vs-Mesen2 split is 1-vs-1 until you check
     it. In #3003 Snes9x is what turned a stand-off into a decision -- it had the rule ares
     had (`line ^ (OBJWidths[S] - 1)`) *and* an explanatory comment ("Yes, Width not
     Height"), which is stronger evidence than a third byuu-derived copy would have been.
     Snes9x is not usually cloned locally; fetch it from
     `raw.githubusercontent.com/snes9xgit/snes9x/master/` (OBJ/sprite logic lives in
     `gfx.cpp`'s `SetupOBJ`, not `ppu.cpp`).
   - **When they disagree**, state both approaches and investigate against hardware specs.
     Worked example (#3011): for CGWSEL clip mode 3 ("always clip main to black") ares
     disables colour-math halving via one uniform rule
     (`colorHalve = io.colorHalve && above.colorEnable`) while Mesen2 clears its
     `halfShift` in the two *windowed* clip branches only. NESER followed ares -- one
     coherent rule beats a special case, and no vendored ROM reaches mode 3 so no golden
     was at stake. Deliberate divergences from the project's designated ground-truth
     emulator must be commented at the call site, so a future Mesen2 cross-check that
     hits the case is not mistaken for a regression.

7. **bsnes / higan (legacy reference)**
   - Repo: `https://github.com/bsnes-emu/bsnes` (or higan)
   - Prefer ares (Near/byuu's current work) over bsnes/higan for newer issues.
   - Still useful for historical context and cross-checking edge cases.

## Reporting rules

- Prefer written specification (fullsnes, anomie) over emulator implementation.
- If only bsnes/higan provides an answer, say the statement is implementation-backed rather than specification-backed.
- If you still cannot confirm the behavior, state that the detail is unknown instead of inferring it.
