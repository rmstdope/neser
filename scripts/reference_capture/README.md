# Reference-emulator screenshot capture

Headless capture recipes for the screenshot reference of each system, verified on
2026-09-25 on macOS (Apple Silicon), the NES and CGB checks on 2026-09-26. Each recipe was
checked end to end: the reference capture and a NESER `--headless` capture of the same ROM
matched pixel-for-pixel through `python -m scripts.diff_screenshots`
(NES: blargg_ppu_tests power_up_palette frame 120 with --nes-palette mesen; GB:
dmg-acid2 after 10 s; CGB: cgb-acid2 after 10 s, both against the SameBoy tester the
recipe builds, with colour correction disabled (see "Colour: DMG and CGB"); GBA: mGBA
suite frame 300, and every frame from 1 to 120 but 9 with `--skip-bios-intro`). The
Mesen2 recipe was re-verified on animated NES and SNES content on 2026-09-27 (nr-mxn; see
"Frame numbering" under Mesen2).

| System | Reference | Tool | Frame-exact |
|---|---|---|---|
| NES, SNES | Mesen2 2.1.1 | `compare_mesen2` (`Mesen --testRunner` + `mesen2_capture.lua`) | yes |
| GB / CGB | SameBoy 1.0.3 | `sameboy_tester` (built from source, patched) | no, time-based |
| GBA | mGBA 0.11 (git) | `mgba-headless` (built from source, patched) + `mgba_capture.lua` | yes |

NESER side, for every system:

```bash
cargo build --release --bin neser
target/release/neser --headless --frames <N> --output neser.png <rom>
# NES: add --nes-palette mesen (Mesen2's colours; without it whites differ)
# GBA with NESER's built-in BIOS: add --skip-bios-intro (see "Frame numbering" under mGBA)
python -m scripts.diff_screenshots neser.png reference.png --shift-search 1
```

`--headless` forces zero-initialised RAM; pin the same on the reference side.

## Mesen2 (NES and SNES)

Binary: `/Applications/Mesen.app/Contents/MacOS/Mesen`, the official release zip from
`https://github.com/SourMesen/Mesen2/releases` (`Mesen_<ver>_macOS_ARM64_AppleSilicon.zip`,
unzip twice: the zip holds `Mesen.app.zip`). Strip the quarantine flag with
`xattr -dr com.apple.quarantine /Applications/Mesen.app`. There is no Homebrew cask.

Lua file I/O is disabled by default and `emu.takeScreenshot()` cannot be saved without it:

```bash
SET="$HOME/Library/Application Support/Mesen2/settings.json"
cp "$SET" "$SET.backup"
sed -i '' 's/"AllowIoOsAccess": false/"AllowIoOsAccess": true/' "$SET"
# ... capture ...
mv "$SET.backup" "$SET"        # restore when done
```

`settings.json` only exists after Mesen2 has been started once (the first testRunner run
creates it, so run the capture twice on a fresh install).

With file access off, Mesen2 leaves the Lua globals `io` and `os` undefined. `mesen2_capture.lua`
checks for that before anything else: it prints one `ERROR: Lua file access is off; set
"AllowIoOsAccess": true ...` line and stops Mesen2 with exit code 1 at once. Without that
check, the run would sit silently until `--timeout` (nr-hg7).

### Comparing a ROM: `compare_mesen2`

Compare a NES or SNES ROM with this one command; sweep beads name it rather than copying flags,
so a confound found later is fixed in the command once:

```bash
cargo build --release --bin neser
python -m scripts.reference_capture.compare_mesen2 <rom.nes|rom.sfc> --frames 120 600 --out-dir out/
```

It prints what each emulator decided about the ROM, then one line per frame, and writes
`mesen2-<N>.png` and `neser-<N>.png` into `--out-dir` (a new temporary directory when it is
left out) for `python -m scripts.diff_screenshots`. Exit status 0 means every frame matched, 1 a
frame differs, 2 a capture failed.

```text
Mesen2: [iNes] Mapper: 0 Sub: 0
Mesen2: [DB] Game not found in database
NESER:  Loaded rom with CRC32: 5CE951EA, mapper=0, submapper=0, PRG-ROM=32KB, CHR-ROM=8KB
NESER:  Hardware: NES (NTSC) | Port 1: Joypad | Port 2: Joypad
frame 120: 0 differing pixels
```

The command owns the flags and removes the confounds earlier comparisons paid for:

- **Battery saves** (nr-kds, nr-nuf, nr-7v3). Both emulators load a save by ROM file name, and
  Mesen2 keeps its saves in `~/Library/Application Support/Mesen2/Saves/`, not beside the ROM.
  A name shared between runs (one `fresh.nes` for every game, or one copy per game reused across
  checkpoints) boots later runs on an earlier run's SRAM. Every invocation of either emulator
  runs on its own copy (`<rom>-<pid>-<nanoseconds>.<ext>`), and Mesen2's `Saves`,
  `RecentGames` and `SaveStates` entries for that copy are deleted afterwards.
- **Mesen2's game database** (nr-nwy, nr-sjt, nr-1le). Mesen2 replaces the iNES header with its
  database entry without saying so on screen, and misses the database for a file with trailing
  data (it hashes everything after the header). The `[iNes]` and `[DB]` lines show which it used:
  a `[DB] Mapper:` that differs from `[iNes] Mapper:`, `Game not found in database` or `File is
  larger than expected` means the two emulators may not be running the same board. Compare
  NESER's `mapper=` before filing a difference; `--no-game-database` makes Mesen2 use the header
  (`--nes.DisableGameDatabase=true`). Mesen2's database is also wrong for some Vs. System PPUs
  (nr-1le): it is not the reference for a Vs. game's palette.
- **Controller ports** (nr-0an). Without port flags Mesen2 takes its ports from
  `settings.json`, where port 2 may be empty, and games that read which pads are connected play
  differently (Super Bomberman 3's attract demo lagged a frame at 2917). Both systems get a
  standard pad in each port (`--nes.portN.type=NesController`,
  `--snes.portN.type=SnesController`), and NESER runs with an empty `--config` file, so a
  `neser.conf` port or palette line cannot apply; NESER's defaults are the same pads. For a
  Mouse or Super Scope game pass the Mesen2 type and NESER's port with `--mesen2-arg` and
  `--neser-arg` (for example `--mesen2-arg=--snes.port2.type=SnesMouse --neser-arg=--snes-controller-port2
  --neser-arg=mouse`).
- **Region** (nr-f6o). NESER's `Hardware:` line names the region it picked; NESER's ROM database
  makes some "NTSC" files PAL or Dendy. Record it per ROM with a sweep's results, so a
  region-gated change is read per region. NESER prints no cartridge lines for SNES ROMs.

What it runs, for reading only: Mesen2 `--testRunner --enableStdout --timeout=30
--Video.VideoFilter=None --Video.AspectRatio=NoStretching`, frame skipping off, zero RAM and the
port flags above, with `CAPTURE_FRAME=N CAPTURE_OUT=<absolute path>` and `mesen2_capture.lua`;
NESER `--config <empty> --headless --frames N` (`--nes-palette mesen` for the NES), whose
`--headless` forces zero RAM. The frame-skip flag is mandatory for animated content:
testRunner emulation runs far faster than real time and otherwise renders only every other
frame. The capture script prints `SAVED <path>` and stops the emulator; one run takes about a
second. Do not run two Mesen2 testRunners at once: the second exits 0 with no output, so the
command waits for any other `Mesen --testRunner` to finish first. `MESEN2_BIN`, `NESER_BIN` and
`MESEN2_HOME` (or `--mesen2-bin`, `--neser-bin`, `--mesen2-home`) point it elsewhere.

Frame numbering: the script counts `startFrame` events from power-on and, at the N-th,
reads the pixels with `emu.getScreenBuffer()`, so `CAPTURE_FRAME=N` is the N-th emulated
frame, the same frame as NESER's `--frames N`. Two easier-looking choices are both wrong
by one frame (nr-mxn):

- `endFrame`: Mesen2 raises it before the PPU sends the frame it has just rendered
  (`SnesPpu.cpp` and `NesPpu.cpp` call `ProcessEvent(EndFrame)` before `SendFrame()`),
  so a screenshot there is always frame N-1. The script did this until nr-mxn.
- `emu.takeScreenshot()`: it copies the output of Mesen2's video-decode thread, which
  `SendFrame()` only signals. On a loaded machine the decode of frame N has not always
  run by `startFrame`, and the screenshot is frame N-1 some of the time (4 of 48
  captures at load average 30-50).

`emu.getScreenBuffer()` filters the PPU's own buffer on the emulation thread, and at
`startFrame` that buffer still holds frame N (the SNES PPU switches buffers at the end
of scanline 0, the NES PPU on the pre-render line, right after raising `StartFrame`). Because it returns pixels,
the script encodes the PNG itself (uncompressed, so a few hundred KB). The buffer is
uncropped: the script keeps the SNES lines 7-230 of 239 (14-461 of 478 in hi-res), the
224 (448) that NESER outputs and Mesen2's default SNES overscan shows, and the NES
buffer's 240 lines as they are. Mesen2's own overscan settings therefore no longer
affect the capture. One setting still does: with the SNES `DeinterlaceMode` set to
`CurrentField`, Mesen2 clears the buffer of an interlaced frame at scanline 240, and the
capture of such a frame is black. The default, `Weave`, is safe; check
`settings.json` if an interlaced capture comes out black. The script is written and
verified for the NES and the SNES only.

A static screen cannot show a one-frame offset, and the recipe had only been checked on
static frames before nr-mxn. Verified 2026-09-27 on animated content, capture N against
NESER N at 0 px and against N±1 not:
`undisbeliever-ppu-window/window-precalculated-single.sfc` (SNES) and
`nmi_sync/demo_ntsc.nes` (NES; it alternates two images each frame, so it pins the
parity only), at 24 frames each, twice at load average 30-50 with no miss. It was also
verified on `mandrill64PerTileRowHiRes.sfc` (512x448) and on the NES static check above.
The check is `NESER_MESEN2_CAPTURE_TEST=1 python -m unittest scripts.test_mesen2_capture`.
It is opt-in because it needs Mesen2 and a release build of the tree under test (the
gate builds none). It leaves `AllowIoOsAccess` alone. The same module's
`TestMesen2CaptureWithoutFileAccess` (nr-hg7) checks the file-access error above. It
simulates the setting being off with a shim that sets `io` and `os` to nil, and needs Mesen2
but no NESER build.
`scripts/test_compare_mesen2.py` pins `compare_mesen2` itself (a ROM copy per Mesen2
invocation, its save removed afterwards, the flags per system and the printed cartridge lines)
against fake emulators, so it runs in the gate with no Mesen2 installed.

## Tracing against Mesen2 (NES and SNES)

For a game that drifts from Mesen2, compare *when* things happen before comparing
pictures. A picture shows only what the game has drawn, and RAM can differ long before
that reaches the screen (nr-046). Two traces do it, each written by both emulators in the
same format and compared by `scripts/diff_timing_traces.py` (nr-ggx):

- **NMI clock log**: one line per NMI entry, `nmi=<n> pc=<handler> clk=<clock>`. A
  CPU-timing drift moves the next NMI entry, so this one line per frame finds the frame in
  seconds. The same answer took over an hour with a per-instruction trace (nr-4lq).
- **Exec trace**: one line per instruction, `pc=<address> clk=<clock>`, between two NMI
  entries: the frame the NMI log pointed at.

The clock on each line is the clock *before the instruction's opcode fetch*: CPU cycles
on the NES, master clocks on the SNES, Mesen2's `masterClock` on both. NESER needs no
patching. `timing_trace` reads the PC, the clock and two trace counters the cores keep
(NMIs taken, instructions executed).

```bash
cargo build --release --features native --bin timing_trace
ROM=<rom>; T="$PWD/trace"; mkdir -p "$T"
MESEN=/Applications/Mesen.app/Contents/MacOS/Mesen
FLAGS="--nes.RamPowerOnState=AllZeros"   # SNES: the four --snes.* flags from "Mesen2" above

# 1. The first NMI entry whose clock differs.
target/release/timing_trace nmi "$ROM" --nmis 3600 --out "$T/neser_nmi.txt"
TRACE_NMIS=3600 TRACE_OUT="$T/mesen_nmi.txt" \
  $MESEN --testRunner --enableStdout --timeout=300 $FLAGS "$ROM" \
  scripts/reference_capture/mesen2_nmi_clock.lua
python -m scripts.diff_timing_traces "$T/neser_nmi.txt" "$T/mesen_nmi.txt"
# -> first divergence at line K: clock offset 6 -> 18   (line K is NMI entry K)

# 2. Every instruction from entry K-1 up to entry K: the instruction that drifted.
target/release/timing_trace exec "$ROM" --from-nmi <K-1> --to-nmi <K> --out "$T/neser_exec.txt"
TRACE_FROM_NMI=<K-1> TRACE_TO_NMI=<K> TRACE_OUT="$T/mesen_exec.txt" \
  $MESEN --testRunner --enableStdout --timeout=300 $FLAGS "$ROM" \
  scripts/reference_capture/mesen2_exec_trace.lua
python -m scripts.diff_timing_traces "$T/neser_exec.txt" "$T/mesen_exec.txt"
```

`--from-nmi 0` traces from power-on. `TRACE_OUT` must be absolute, and the scripts need
`"AllowIoOsAccess": true` as `mesen2_capture.lua` does. `timing_trace` reads no
`neser.conf`: it runs with default settings and zero-filled RAM, which is what the Mesen2
flags pin. A per-instruction Mesen2 trace runs at roughly 250k SNES master clocks a
second, about a second and a half per SNES frame, so keep the exec window to the frames
the NMI log points at.

**Reading the diff.** Only a *change* in the clock offset means anything. By default the
first line's offset is the baseline, so a drift that happened before a trace's first line
is invisible: before NMI 1 for an NMI log, before entry K for `exec --from-nmi K`. Pass
`--baseline` when you know the expected offset. It is 0 on the NES, where NESER's CPU cycle
count and Mesen2's `masterClock` start together. On the SNES it is the offset of an
`exec --from-nmi 0` trace, 0 on every ROM traced so far. A trace whose first line is
already off that baseline has drifted earlier, so look before it.

One known SNES difference: on `undisbeliever-ppu-window/window-precalculated-single.sfc`,
the instructions match at offset 0 from power-on, but every NMI entry is 6 master clocks
later in NESER than in Mesen2 (2026-10-04, nr-7pk). Its NMI log diffs clean with the
default baseline and stops at line 1 with `--baseline 0`.

In an exec trace, an offset that leaves the baseline for a single line and comes straight
back is listed as a *stamp difference*, never a divergence. A stall that falls on an
instruction boundary is charged to the instruction before it by one emulator and to the
one after it by the other, and the totals still agree. Three cases are known:

- NES OAM DMA after a `$4014` write: Mesen2 stamps the next instruction before the
  513/514-cycle stall, NESER after it.
- SNES DRAM refresh (40 master clocks) at the start of an opcode fetch.
- An SNES WAI woken with the interrupt masked: NESER runs the two wake cycles and the next
  instruction in one step, so it stamps that instruction 2 CPU cycles early.

In an NMI log every line is a frame, so a single late entry there *is* a divergence.

**Mesen2 Lua behaviours** (Mesen2 2.1.1), each one paid for by a bead:

- `emu.addMemoryCallback(cb, type, lo, hi, emu.cpuType.snes)` never fires. The same call
  without the `cpuType` argument does (nr-dh7).
- A memory callback on `$2100-$21FF` sees bank `$00` only. Writes through bank `$80`
  (FastROM) or with DBR=`$81` are missing, which looks like NESER writes Mesen2 never made
  (nr-dh7).
- `read` callbacks never fire on PPU registers (`$2137-$213F`, with or without the bank or
  `cpuType`/`memType`). Sample the register in an `exec` callback on the instruction after the
  load instead, by reading `cpu.a` (nr-4cl).
- An `exec` callback fires once per instruction, at the opcode fetch, with the full 24-bit
  address on the SNES. `emu.getState().masterClock` inside it is the clock before that
  fetch, the point `timing_trace` samples (nr-4cl, nr-ggx).
- `emu.eventType.nmi` fires when the NMI is raised, a few cycles before the CPU enters the
  handler, so the scripts use it only to arm an `exec` callback on the handler, which they
  read from the vector with `emu.read`. `emu.read` is a debugger read with no side effects.
  A CPU read of `$FFFA` is watched by some mappers (MMC5).
- `endFrame`, `startFrame` and NESER's ready-to-render point all sit at different places in
  the frame. `endFrame` and `is_ready_to_render()` fall on opposite sides of the writes at
  the start of SNES scanline 225 (nr-dh7), and WRAM dumps taken at each emulator's own frame
  event showed a one-frame lag that was not there (nr-4lq). Compare by NMI entry or by clock,
  never per frame event.
- The NMI entry is the only synchronous sampling point when a game spins an RNG in its idle
  loop: any mid-frame RAM sample differs between the emulators (nr-046).
- `emu.getState()` builds the whole console state on every call, so calling it per
  instruction is slow. Find the frame with the NMI log first (nr-4lq).
- `emu.getState()`'s APU fields are stale mid-frame: Mesen2 runs the APU lazily, catching up
  only at register accesses, the frame end or an IRQ. Time an APU event with a memory
  callback on the event itself (a `read` callback on the DMC sample range gives each fetch's
  `cpu.cycleCount`), never with a state snapshot (nr-6gs).
- `emu.stop()` is not immediate. Callbacks keep firing for a while after it, so a script
  that stops after its last line must ignore them (nr-ggx).
- Two `--testRunner` runs at once make one exit 0 with no output. Run them one at a time.

The check that both halves agree is `NESER_MESEN2_TRACE_TEST=1 python -m unittest
scripts.test_mesen2_traces`. It is opt-in like `test_mesen2_capture` and needs Mesen2 and a
release `timing_trace`. It diffs the NMI log and two exec windows of
`nmi_sync/demo_ntsc.nes` and `window-precalculated-single.sfc` and expects no divergence,
and it checks that both scripts stop at once, naming the setting, when Lua file access is
off.

## SameBoy (GB and CGB)

The SameBoy.app cask has no command-line mode. Its repository ships a headless
`Tester/` frontend that runs a ROM for a number of seconds and writes a BMP. Build it with
one small patch that turns off its colour correction (why: "Colour: DMG and CGB"):

```bash
git clone --depth 1 https://github.com/LIJI32/SameBoy ~/repos/SameBoy
cd ~/repos/SameBoy && git apply <neser>/scripts/reference_capture/sameboy-tester-no-color-correction.patch
make -k build/bin/tester/sameboy_tester -j8
```

On an existing clone, run `git checkout Tester/main.c && git pull` first. `git apply`
refuses loudly if upstream moved the line (or if the patch is already applied). The patch
replaces the tester's hard-coded `GB_COLOR_CORRECTION_EMULATE_HARDWARE` in
`GB_set_color_correction_mode` with `GB_COLOR_CORRECTION_DISABLED` (checked against
upstream 213a12c); the tester has no command-line flag for it.

Without rgbds the boot-ROM step fails; `-k` and the binary as target let the tester link
anyway (plain `make tester -j8` stops at the boot-ROM error on a fresh tree).

Result: `~/repos/SameBoy/build/bin/tester/sameboy_tester`. Because the bundled boot ROMs
did not build, point `--boot` at the ones inside `/Applications/SameBoy.app` (installed
with `brew install --cask sameboy`):

```bash
cp <rom.gb> work/            # the BMP lands next to the ROM, as <rom>.bmp
cd work && ~/repos/SameBoy/build/bin/tester/sameboy_tester \
  --dmg --length 10 --boot /Applications/SameBoy.app/Contents/Resources/dmg_boot.bin <rom.gb>
python -c "from PIL import Image; Image.open('<rom>.bmp').convert('RGB').save('sameboy.png')"
```

Models: `--dmg` (DMG-B), `--sgb` (SGB2), default is CGB-E with `cgb_boot.bin`. The tester
disables SameBoy's randomisation (`GB_random_set_enabled(false)`), so captures are
reproducible. Do not pass `--start`; it makes the tester press START/A on a schedule.

Caveat, time-based not frame-based: `--length N` is N seconds measured in blocks of
139810 emulated cycles, and the tester keeps running until the screen is non-blank (up to
four seconds more). NESER's DMG boot animation also takes longer than SameBoy's boot ROM,
so early captures diverge on timing alone (dmg-acid2 at 120/180/240 NESER frames vs 2/3/4 s
of SameBoy differ by 44 %; at 600 frames vs 10 s they are identical). Use SameBoy only for
screens that are static by the capture time, and prove it by capturing at two lengths that
give byte-identical BMPs before trusting the diff.

### Colour: DMG and CGB

Measured 2026-09-26 (nr-x4l), NESER `--headless --frames 600` against `sameboy_tester
--length 10`, both stable (NESER 600 = 720 frames, SameBoy 10 s = 12 s, byte-identical):

| ROM | SameBoy model | Differing pixels | Cause |
|---|---|---|---|
| dmg-acid2.gb | `--dmg` (DMG-B) | 0 / 23040 | none: both use greys 0, 85, 170, 255 |
| cgb-acid2.gbc | default (CGB-E) | 6814 / 23040 (29.6 %) | SameBoy's colour correction only |

That table is the stock tester. The CGB difference is colour alone. Every NESER colour
maps to exactly one SameBoy colour (e.g. (255,255,0) → (255,213,0), (0,0,255) →
(0,107,255), (107,189,255) → (125,233,255)): NESER expands RGB555 as `(c << 3) | (c >> 2)`,
the formula cgb-acid2 specifies, while the stock tester hard-codes
`GB_COLOR_CORRECTION_EMULATE_HARDWARE` (a deprecated alias of
`GB_COLOR_CORRECTION_MODERN_BALANCED`). With the patched tester the recipe builds, measured
2026-09-26 (nr-y3e), both ROMs match: cgb-acid2 0 / 23040 and dmg-acid2 0 / 23040 (DMG
output never passes through the correction, so the patch leaves it unchanged).

The CGB capture itself (default model CGB-E; the BMP lands next to the ROM):

```bash
cp roms/gb/automated_tests/acid/cgb-acid2.gbc work/
cd work && ~/repos/SameBoy/build/bin/tester/sameboy_tester \
  --length 10 --boot /Applications/SameBoy.app/Contents/Resources/cgb_boot.bin cgb-acid2.gbc
# NESER, from the repo root:
target/release/neser --headless --frames 600 --output neser.png roms/gb/automated_tests/acid/cgb-acid2.gbc
```

**Why the reference is patched rather than NESER.** NESER's CGB output stays the raw
expansion: it is the formula cgb-acid2 specifies for its reference image, so NESER's
captures stay comparable against the test ROM's own expectation. NESER's optional CGB
colour correction (nr-1gg) is a different curve from SameBoy's, and nr-1gg's design
rejected SameBoy's "modern balanced" look, so NESER does not imitate it. Leave any CGB
colour correction off for reference captures: headless captures apply it when it is on.
Turning the correction off on the reference side is the one change that makes both sides
agree (decided by the navigator, nr-y3e).

## mGBA (GBA)

`/Applications/mGBA.app` and the Homebrew `mgba` formula are the Qt GUI, which takes no
script on its command line (the Homebrew 0.10.5 bottle also fails to start against ffmpeg
9, `libavcodec.62.dylib` missing). Build mGBA's own headless tool from source instead,
with Lua scripting and one small patch:

```bash
git clone --depth 1 https://github.com/mgba-emu/mgba ~/repos/mgba
cd ~/repos/mgba && git apply <neser>/scripts/reference_capture/mgba-headless-video-buffer.patch
mkdir build && cd build
cmake .. -DBUILD_QT=OFF -DBUILD_SDL=OFF -DBUILD_HEADLESS=ON -DENABLE_SCRIPTING=ON -DUSE_LUA=ON \
         -DUSE_FFMPEG=OFF -DBUILD_GL=OFF -DBUILD_GLES2=OFF -DCMAKE_BUILD_TYPE=Release
make -j8 mgba-headless        # needs brew: cmake lua libpng libzip libepoxy
```

The patch gives the core a framebuffer. Stock `mgba-headless` never renders, so
`emu:screenshot()` writes a 33-byte PNG holding only the header.

```bash
CAPTURE_FRAME=300 CAPTURE_OUT="$PWD/mgba.png" \
  ~/repos/mgba/build/mgba-headless --script scripts/reference_capture/mgba_capture.lua <rom.gba>
```

No BIOS file is passed, so mGBA uses its HLE BIOS. Pass `-b <bios.bin>` on the mGBA side
and the equivalent NESER option only when the ROM under test exercises the real BIOS.

Frame numbering. Both sides count frames from power-on (`emu:currentFrame()` and NESER's
`--frames`), but they do not reach the cartridge at the same frame:

- **Pass `--skip-bios-intro` on the NESER side when it runs its built-in BIOS** (the
  default, and what matches mGBA's HLE BIOS). NESER's built-in BIOS plays its
  logo and jingle by default, which delays the cartridge by about 255 frames (the mGBA
  suite draws its menu at frame 9 in mGBA and at frame ~265 in NESER without the flag).
  mGBA's HLE BIOS has no intro and jumps straight to the cartridge. With the flag, NESER
  runs only the BIOS hardware init. Do not pass it with a real BIOS image (`-b` on the
  mGBA side, `--gba-bios-path` on NESER's): both sides then play the real intro, and the
  flag only writes 1 to IWRAM 0x03007FFC, the user IRQ-handler pointer, so RAM is no
  longer zero-initialised.
- **Expect one frame of offset while a ROM is still drawing its first screen.** mGBA's
  no-BIOS boot (`GBASkipBIOS`) sets VCOUNT to 126 at cartridge entry, so its first frame is
  only 102 of 228 lines long; NESER enters the cartridge near line 0 after running its
  BIOS init. The mGBA suite matches pixel for pixel at every frame from 1 to 120 except
  frame 9, where mGBA has already finished drawing the menu and NESER finishes at frame 10.
  Compare at a frame where the screen is static (capture two consecutive frames on each
  side and check they are identical).

Before the cartridge writes DISPCNT, both show a white screen (forced blank, DISPCNT =
0x0080).
