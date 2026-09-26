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
| NES, SNES | Mesen2 2.1.1 | `Mesen --testRunner` + `mesen2_capture.lua` | yes |
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

```bash
CAPTURE_FRAME=120 CAPTURE_OUT="$PWD/mesen.png" \
  /Applications/Mesen.app/Contents/MacOS/Mesen --testRunner --enableStdout --timeout=30 \
  --Video.VideoFilter=None --Video.AspectRatio=NoStretching \
  --nes.DisableFrameSkipping=true --nes.RamPowerOnState=AllZeros \
  <rom.nes> scripts/reference_capture/mesen2_capture.lua
```

For the SNES replace the two `--nes.*` flags with `--snes.disableFrameSkipping=true
--snes.RamPowerOnState=AllZeros`. The frame-skip flag is mandatory for animated content:
testRunner emulation runs far faster than real time and otherwise renders only every other
frame. `CAPTURE_OUT` must be absolute. The script prints `SAVED <path>` and stops the
emulator; the whole run takes about a second. The stdout log also lists the mapper,
CRCs and any uninitialised-memory reads, which is useful in itself.

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
gate builds none). It leaves `AllowIoOsAccess` alone.

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
