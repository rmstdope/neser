"""Frame alignment of scripts/reference_capture/mesen2_capture.lua against NESER (nr-mxn).

The committed Mesen2 capture script must save the same frame as ``neser --headless
--frames N`` for ``CAPTURE_FRAME=N``. A static test ROM cannot show a one-frame offset,
so each case is a ROM that changes every frame: the capture must equal NESER frame N at
0 px and differ from frames N-1 and N+1.

This needs the Mesen2 release binary and a release NESER build, so it is skipped where
either is missing (CI has no Mesen2). Point ``MESEN2_BIN`` / ``NESER_BIN`` at them when
they are not in the default places (``/Applications/Mesen.app`` and
``target/release/neser``).

The committed script is run unchanged except for a prepended shim. The shim replaces
``io.open`` so the PNG reaches stdout as hex and ``os.getenv`` so the frame comes from
the test. Mesen2's own ``AllowIoOsAccess`` setting, which is global and shared with
every other session, is left alone.
"""

import os
import shutil
import subprocess
import tempfile
import time
import unittest
from pathlib import Path

from scripts.diff_screenshots import Screenshot, diff_pixels, load_screenshot

REPO = Path(__file__).resolve().parent.parent
SCRIPT = REPO / "scripts" / "reference_capture" / "mesen2_capture.lua"
MESEN2 = Path(os.environ.get("MESEN2_BIN", "/Applications/Mesen.app/Contents/MacOS/Mesen"))
NESER = Path(os.environ.get("NESER_BIN", str(REPO / "target" / "release" / "neser")))

SHIM = """
io = { open = function(path, mode)
  return { write = function(self, data)
      local hex = {}
      for i = 1, #data do hex[i] = string.format("%02X", string.byte(data, i)) end
      print("PNG_HEX " .. table.concat(hex))
    end, close = function(self) end }
end }
os = os or {}
os.getenv = function(k)
  if k == "CAPTURE_FRAME" then return "__FRAME__" end
  if k == "CAPTURE_OUT" then return "stdout" end
end
"""

# (rom, NESER flags, Mesen2 flags, frame) for content that changes every frame.
NES_FLAGS = ["--nes.DisableFrameSkipping=true", "--nes.RamPowerOnState=AllZeros"]
SNES_FLAGS = ["--snes.disableFrameSkipping=true", "--snes.RamPowerOnState=AllZeros"]
CASES = [
    (
        # Alternates two images every frame, so it pins the parity of the frame; no NES
        # ROM in roms/ changes monotonically and also matches Mesen2 at 0 px.
        "roms/nes/automated_tests/nmi_sync/demo_ntsc.nes",
        ["--nes-palette", "mesen"],
        NES_FLAGS,
        120,
    ),
    (
        "roms/snes/automated_tests/snes_test_roms/undisbeliever-ppu-window/"
        "window-precalculated-single.sfc",
        [],
        SNES_FLAGS,
        120,
    ),
]


def neser_frame(rom: Path, flags: list[str], frame: int, out_dir: Path) -> Screenshot:
    out = out_dir / f"neser_{frame}.png"
    cmd = [str(NESER), "--headless", "--frames", str(frame), "--output", str(out), *flags]
    subprocess.run([*cmd, str(rom)], check=True, capture_output=True, timeout=120)
    return load_screenshot(out)


def wait_for_other_mesen2() -> None:
    """A concurrent testRunner makes Mesen2 exit 0 with no output (snes-hardware-research)."""
    deadline = time.monotonic() + 300
    while subprocess.run(["pgrep", "-f", "Mesen --testRunner"], capture_output=True).returncode == 0:
        if time.monotonic() > deadline:
            raise TimeoutError("another Mesen --testRunner is still running")
        time.sleep(2)


def mesen2_capture(rom: Path, flags: list[str], frame: int, out_dir: Path) -> Screenshot:
    script = out_dir / "capture.lua"
    script.write_text(SHIM.replace("__FRAME__", str(frame)) + SCRIPT.read_text())
    wait_for_other_mesen2()
    cmd = [str(MESEN2), "--testRunner", "--enableStdout", "--timeout=30"]
    cmd += ["--Video.VideoFilter=None", "--Video.AspectRatio=NoStretching", *flags]
    run = subprocess.run([*cmd, str(rom), str(script)], capture_output=True, text=True, timeout=120)
    hex_lines = [line for line in run.stdout.splitlines() if line.startswith("PNG_HEX ")]
    if len(hex_lines) != 1:
        raise AssertionError(f"Mesen2 printed no screenshot; stdout was:\n{run.stdout}")
    png = out_dir / f"mesen2_{frame}.png"
    png.write_bytes(bytes.fromhex(hex_lines[0].removeprefix("PNG_HEX ")))
    return load_screenshot(png)


@unittest.skipUnless(MESEN2.is_file() and shutil.which("pgrep"), "Mesen2 binary not found")
@unittest.skipUnless(NESER.is_file(), "release NESER binary not found (cargo build --release)")
class TestMesen2CaptureFrameAlignment(unittest.TestCase):
    def test_capture_frame_n_is_neser_frame_n_on_animated_content(self) -> None:
        for rom_path, neser_flags, mesen2_flags, frame in CASES:
            with self.subTest(rom=Path(rom_path).name), tempfile.TemporaryDirectory() as tmp:
                rom, out_dir = REPO / rom_path, Path(tmp)
                capture = mesen2_capture(rom, mesen2_flags, frame, out_dir)
                diffs = {
                    n: diff_pixels(neser_frame(rom, neser_flags, n, out_dir), capture)
                    for n in (frame - 1, frame, frame + 1)
                }
                self.assertEqual(diffs[frame], 0, f"differing pixels by NESER frame: {diffs}")
                self.assertNotEqual(diffs[frame - 1], 0, "content must animate to prove alignment")
                self.assertNotEqual(diffs[frame + 1], 0, "content must animate to prove alignment")


if __name__ == "__main__":
    unittest.main()
