"""The Mesen2 trace scripts and NESER's `timing_trace` agree line for line (nr-ggx).

`scripts/reference_capture/mesen2_nmi_clock.lua` and `mesen2_exec_trace.lua` are run
against `timing_trace nmi` and `timing_trace exec` on one NES and one SNES ROM that NESER
emulates clock-exactly. `scripts.diff_timing_traces` must find no divergence. That proves the
two halves sample the same point (the clock before an instruction's opcode fetch) and count
NMI entries the same way. A timing regression in NESER also fails it.

Opt-in like `test_mesen2_capture`: set ``NESER_MESEN2_TRACE_TEST=1``. It needs the Mesen2
release binary and a release build of the tree under test (``cargo build --release
--features native --bin timing_trace``); ``MESEN2_BIN`` / ``TIMING_TRACE_BIN`` override the
default paths. The scripts run unchanged behind a shim that sends ``io.open`` writes to
stdout and supplies ``os.getenv``, so Mesen2's shared ``AllowIoOsAccess`` setting is left alone.
"""

import os
import shutil
import subprocess
import tempfile
import time
import unittest
from pathlib import Path

from scripts.diff_timing_traces import diff_traces, format_report, parse_lines
from scripts.test_mesen2_capture import MESEN2, NES_FLAGS, NO_FILE_ACCESS_SHIM, SNES_FLAGS, wait_for_other_mesen2

REPO = Path(__file__).resolve().parent.parent
LUA = REPO / "scripts" / "reference_capture"
TIMING_TRACE = Path(os.environ.get("TIMING_TRACE_BIN", str(REPO / "target" / "release" / "timing_trace")))

# Lines written through io.open reach stdout prefixed with TRACE; os.getenv answers from ENV.
SHIM = """
local env = { __ENV__ }
io = { open = function(path, mode)
  return { write = function(self, data)
      for line in data:gmatch("[^\\n]+") do print("TRACE " .. line) end
    end, close = function(self) end }
end }
os = os or {}
os.getenv = function(k) return env[k] end
"""

NES_ROM = "roms/nes/automated_tests/nmi_sync/demo_ntsc.nes"
SNES_ROM = "roms/snes/automated_tests/snes_test_roms/undisbeliever-ppu-window/window-precalculated-single.sfc"
ROMS = [(NES_ROM, NES_FLAGS), (SNES_ROM, SNES_FLAGS)]


def mesen2_trace(script: str, rom: Path, flags: list[str], env: dict[str, str], tmp: Path) -> list[str]:
    values = {"TRACE_OUT": "stdout", **env}
    lua = tmp / script
    entries = ", ".join(f'{key} = "{value}"' for key, value in values.items())
    lua.write_text(SHIM.replace("__ENV__", entries) + (LUA / script).read_text())
    wait_for_other_mesen2()
    cmd = [str(MESEN2), "--testRunner", "--enableStdout", "--timeout=60", *flags, str(rom), str(lua)]
    run = subprocess.run(cmd, capture_output=True, text=True, timeout=180)
    lines = [line.removeprefix("TRACE ") for line in run.stdout.splitlines() if line.startswith("TRACE ")]
    if not lines:
        raise AssertionError(f"Mesen2 wrote no trace; stdout was:\n{run.stdout}")
    return lines


def neser_trace(args: list[str], tmp: Path) -> list[str]:
    out = tmp / "neser.txt"
    subprocess.run([str(TIMING_TRACE), *args, "--out", str(out)], check=True, capture_output=True, timeout=180)
    return out.read_text().splitlines()


@unittest.skipUnless(os.environ.get("NESER_MESEN2_TRACE_TEST") == "1", "opt-in, see docstring")
@unittest.skipUnless(MESEN2.is_file() and shutil.which("pgrep"), "Mesen2 binary not found")
@unittest.skipUnless(TIMING_TRACE.is_file(), "release timing_trace not found (see docstring)")
class TestMesen2TracesLineUpWithNeser(unittest.TestCase):
    def assert_match(self, neser: list[str], mesen2: list[str]) -> None:
        a, b = parse_lines(neser), parse_lines(mesen2)
        result = diff_traces(a, b)
        self.assertIsNone(result.divergence, format_report(a, b, result, context=5))

    def test_nmi_clock_logs_match(self) -> None:
        for rom_path, flags in ROMS:
            with self.subTest(rom=Path(rom_path).name), tempfile.TemporaryDirectory() as tmp:
                rom = REPO / rom_path
                mesen2 = mesen2_trace("mesen2_nmi_clock.lua", rom, flags, {"TRACE_NMIS": "120"}, Path(tmp))
                self.assertEqual(len(mesen2), 120)
                self.assert_match(neser_trace(["nmi", str(rom), "--nmis", "120"], Path(tmp)), mesen2)

    def test_exec_traces_match_from_power_on_and_from_an_nmi(self) -> None:
        for rom_path, flags in ROMS:
            for start in (0, 3):
                with (
                    self.subTest(rom=Path(rom_path).name, from_nmi=start),
                    tempfile.TemporaryDirectory() as tmp,
                ):
                    rom = REPO / rom_path
                    env = {"TRACE_FROM_NMI": str(start)}
                    mesen2 = mesen2_trace("mesen2_exec_trace.lua", rom, flags, env, Path(tmp))
                    neser = neser_trace(["exec", str(rom), "--from-nmi", str(start)], Path(tmp))
                    self.assert_match(neser, mesen2)


@unittest.skipUnless(os.environ.get("NESER_MESEN2_TRACE_TEST") == "1", "opt-in, see docstring")
@unittest.skipUnless(MESEN2.is_file() and shutil.which("pgrep"), "Mesen2 binary not found")
class TestMesen2TracesWithoutFileAccess(unittest.TestCase):
    def test_each_script_stops_at_load_naming_the_setting(self) -> None:
        for script in ("mesen2_nmi_clock.lua", "mesen2_exec_trace.lua"):
            with self.subTest(script=script), tempfile.TemporaryDirectory() as tmp:
                lua = Path(tmp) / script
                lua.write_text(NO_FILE_ACCESS_SHIM + (LUA / script).read_text())
                wait_for_other_mesen2()
                cmd = [str(MESEN2), "--testRunner", "--enableStdout", "--timeout=30", str(REPO / NES_ROM), str(lua)]
                start = time.monotonic()
                run = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
                elapsed = time.monotonic() - start
                named = [line for line in run.stdout.splitlines() if "AllowIoOsAccess" in line]
                self.assertEqual(len(named), 1, run.stdout)
                self.assertEqual(run.returncode, 1)
                self.assertLess(elapsed, 25)


if __name__ == "__main__":
    unittest.main()
