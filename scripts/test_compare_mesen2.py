"""Unit tests for scripts/reference_capture/compare_mesen2.py (nr-ocx).

The command owns the Mesen2 comparison recipe, so these tests pin the confounds earlier
comparisons paid for: battery saves leaking between Mesen2 runs of one ROM name (nr-nuf,
nr-7v3), Mesen2's game database silently overriding the header (nr-nwy, nr-sjt), controller
ports taken from Mesen2's local settings (nr-0an) and the region NESER picked going unrecorded
(nr-f6o). The end-to-end cases run the command against fake ``Mesen`` and ``neser``
executables, so no emulator is needed.
"""

import contextlib
import io
import os
import stat
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

from scripts.reference_capture.compare_mesen2 import (
    NES_MESEN2_FLAGS,
    SNES_MESEN2_FLAGS,
    fresh_copy,
    main,
    merge_flags,
    mesen2_cartridge_lines,
    mesen2_flags,
    neser_cartridge_lines,
    remove_mesen2_leftovers,
    system_for,
    testrunner_pattern,
)

# Real Mesen2 2.1.1 --testRunner --enableStdout output, captured 2026-10-04.
MESEN2_NES_STDOUT = """------------------------------------------------------
Loading rom: demo_ntsc.nes
File CRC32: 0xA2347476
------------------------------------------------------
[DB] Initialized - 10655 games in DB
PRG CRC32: 0xD76717F5
PRG+CHR CRC32: 0x5CE951EA
[iNes] Mapper: 0 Sub: 0
[iNes] PRG ROM: 32 KB
[iNes] CHR ROM: 8 KB
[iNes] Mirroring: Horizontal
[iNes] Battery: No
[DB] Game not found in database
[CPU] Uninitialized memory read: $0300
SAVED /tmp/out.png
"""

MESEN2_SNES_STDOUT = """-----------------------------
File: window-precalculated-single.sfc
Game: PRECALCULATED WINDOW
Type: LoROM
FastROM
Map Mode: $30
Rom Type: $00
File size: 128 KB
ROM size: 128 KB
-----------------------------
SAVED /tmp/out.png
"""

# Real NESER --headless output, captured 2026-10-04.
NESER_NES_OUTPUT = """Loaded rom with CRC32: 5CE951EA, mapper=0, submapper=0, PRG-ROM=32KB, CHR-ROM=8KB
Hardware: NES (NTSC) | Port 1: Joypad | Port 2: Joypad
Saved screenshot to out.png
"""


class TestMesen2Flags(unittest.TestCase):
    def test_nes_flags_pin_ports_and_zero_ram(self) -> None:
        for flag in (
            "--nes.DisableFrameSkipping=true",
            "--nes.RamPowerOnState=AllZeros",
            "--nes.port1.type=NesController",
            "--nes.port2.type=NesController",
        ):
            self.assertIn(flag, NES_MESEN2_FLAGS)

    def test_snes_flags_pin_standard_pads(self) -> None:
        for flag in (
            "--snes.disableFrameSkipping=true",
            "--snes.RamPowerOnState=AllZeros",
            "--snes.port1.type=SnesController",
            "--snes.port2.type=SnesController",
        ):
            self.assertIn(flag, SNES_MESEN2_FLAGS)

    def test_flags_follow_the_system(self) -> None:
        self.assertEqual(mesen2_flags("nes", no_game_database=False), NES_MESEN2_FLAGS)
        self.assertEqual(mesen2_flags("snes", no_game_database=False), SNES_MESEN2_FLAGS)

    def test_no_game_database_only_for_nes(self) -> None:
        self.assertIn("--nes.DisableGameDatabase=true", mesen2_flags("nes", no_game_database=True))
        self.assertNotIn("--nes.DisableGameDatabase=true", mesen2_flags("nes", no_game_database=False))
        self.assertEqual(mesen2_flags("snes", no_game_database=True), SNES_MESEN2_FLAGS)

    def test_an_extra_flag_replaces_the_pinned_flag_with_its_key(self) -> None:
        merged = merge_flags(SNES_MESEN2_FLAGS, ["--snes.port2.type=SnesMouse", "--snes.Overclock=1"])
        self.assertIn("--snes.port2.type=SnesMouse", merged)
        self.assertNotIn("--snes.port2.type=SnesController", merged)
        self.assertIn("--snes.port1.type=SnesController", merged)
        self.assertIn("--snes.Overclock=1", merged)

    def test_concurrent_testrunner_is_matched_by_basename(self) -> None:
        # Another session may start Mesen2 through PATH or a symlink (nr-ocx review).
        self.assertEqual(testrunner_pattern(Path("/Applications/Mesen.app/Contents/MacOS/Mesen")), "Mesen --testRunner")

    def test_system_for_extension(self) -> None:
        self.assertEqual(system_for(Path("a/game.nes")), "nes")
        self.assertEqual(system_for(Path("a/GAME.NES")), "nes")
        self.assertEqual(system_for(Path("a/game.sfc")), "snes")
        self.assertEqual(system_for(Path("a/game.smc")), "snes")
        with self.assertRaises(ValueError):
            system_for(Path("a/game.gb"))


class TestFreshCopies(unittest.TestCase):
    def test_each_copy_has_its_own_name(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            rom = Path(tmp) / "zelda.nes"
            rom.write_bytes(b"NES\x1a rom")
            first, second = fresh_copy(rom, Path(tmp)), fresh_copy(rom, Path(tmp))
            self.assertNotEqual(first.name, second.name)
            for copy in (first, second):
                self.assertEqual(copy.suffix, ".nes")
                self.assertEqual(copy.read_bytes(), rom.read_bytes())

    def test_leftovers_removed_only_for_that_copy(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            copy = home / "zelda-1-2.nes"
            for sub, name in (
                ("Saves", "zelda-1-2.sav"),
                ("Saves", "zelda-1-2.srm"),
                ("RecentGames", "zelda-1-2.rgd"),
                ("SaveStates", "zelda-1-2_1.mss"),
                ("Saves", "zelda.sav"),
                ("Saves", "zelda-1-23.sav"),
            ):
                (home / sub).mkdir(exist_ok=True)
                (home / sub / name).write_bytes(b"x")
            remove_mesen2_leftovers(home, copy)
            left = sorted(p.name for p in home.rglob("*") if p.is_file())
            self.assertEqual(left, ["zelda-1-23.sav", "zelda.sav"])

    def test_leftovers_tolerates_missing_directories(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            remove_mesen2_leftovers(Path(tmp), Path(tmp) / "x-1-2.nes")


class TestCartridgeLines(unittest.TestCase):
    def test_mesen2_nes_lines(self) -> None:
        self.assertEqual(
            mesen2_cartridge_lines(MESEN2_NES_STDOUT),
            [
                "PRG+CHR CRC32: 0x5CE951EA",
                "[iNes] Mapper: 0 Sub: 0",
                "[iNes] PRG ROM: 32 KB",
                "[iNes] CHR ROM: 8 KB",
                "[iNes] Mirroring: Horizontal",
                "[iNes] Battery: No",
                "[DB] Game not found in database",
            ],
        )

    def test_mesen2_snes_header_block(self) -> None:
        lines = mesen2_cartridge_lines(MESEN2_SNES_STDOUT)
        self.assertEqual(lines[0], "Game: PRECALCULATED WINDOW")
        self.assertIn("Type: LoROM", lines)
        self.assertNotIn("File: window-precalculated-single.sfc", lines)
        self.assertFalse(any(line.startswith("---") or line.startswith("SAVED") for line in lines))

    def test_neser_lines(self) -> None:
        self.assertEqual(
            neser_cartridge_lines(NESER_NES_OUTPUT),
            [
                "Loaded rom with CRC32: 5CE951EA, mapper=0, submapper=0, PRG-ROM=32KB, CHR-ROM=8KB",
                "Hardware: NES (NTSC) | Port 1: Joypad | Port 2: Joypad",
            ],
        )


# A fake Mesen2: logs its arguments, writes a battery save named after the ROM into the
# fake Mesen2 home (as the real one does on exit), prints its cartridge lines and writes a
# 2x1 PNG to CAPTURE_OUT.
FAKE_MESEN2 = textwrap.dedent(
    """\
    #!{python}
    import os, sys
    from pathlib import Path
    from PIL import Image
    log, home = Path(os.environ["FAKE_LOG"]), Path(os.environ["FAKE_MESEN2_HOME"])
    rom = Path(next(a for a in sys.argv[1:] if not a.startswith("--") and not a.endswith(".lua")))
    with log.open("a") as f:
        f.write("mesen2 " + " ".join(sys.argv[1:]) + "\\n")
        f.write("saves-before " + " ".join(sorted(p.name for p in (home / "Saves").glob("*"))) + "\\n")
    if os.environ.get("FAKE_LUA_ERROR"):
        print('ERROR: Lua file access is off; set "AllowIoOsAccess": true')
        sys.exit(1)
    (home / "Saves").mkdir(exist_ok=True)
    (home / "Saves" / (rom.stem + ".sav")).write_bytes(b"sram")
    print("[DB] Initialized - 1 games in DB")
    print("[iNes] Mapper: 4 Sub: 0")
    print("[DB] Game found in database")
    print("[DB] Mapper: 118 Sub: 0")
    Image.new("RGB", (2, 1), (1, 2, 3)).save(os.environ["CAPTURE_OUT"])
    print("SAVED " + os.environ["CAPTURE_OUT"])
    """
)

# A fake NESER: logs its arguments, prints its cartridge lines and writes a 2x1 PNG whose
# second pixel differs when FAKE_NESER_DIFFERS is set.
FAKE_NESER = textwrap.dedent(
    """\
    #!{python}
    import os, sys
    from PIL import Image
    args = sys.argv[1:]
    with open(os.environ["FAKE_LOG"], "a") as f:
        f.write("neser " + " ".join(args) + "\\n")
    print("Loaded rom with CRC32: 12345678, mapper=4, submapper=0, PRG-ROM=128KB, CHR-ROM=128KB")
    print("Hardware: NES (PAL) | Port 1: Joypad | Port 2: Joypad")
    img = Image.new("RGB", (2, 1), (1, 2, 3))
    if os.environ.get("FAKE_NESER_DIFFERS"):
        img.putpixel((1, 0), (9, 9, 9))
    img.save(args[args.index("--output") + 1])
    """
)


class TestCompareMesen2EndToEnd(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        root = Path(self.tmp.name)
        self.home = root / "mesen2-home"
        (self.home / "Saves").mkdir(parents=True)
        (self.home / "Saves" / "other-game.sav").write_bytes(b"keep")
        self.log = root / "log.txt"
        self.out = root / "out"
        self.rom = root / "roms" / "kirby.nes"
        self.rom.parent.mkdir()
        self.rom.write_bytes(b"NES\x1a")
        self.mesen2 = self._executable(root / "Mesen", FAKE_MESEN2)
        self.neser = self._executable(root / "neser", FAKE_NESER)
        self.env = {"FAKE_LOG": str(self.log), "FAKE_MESEN2_HOME": str(self.home)}

    def _executable(self, path: Path, source: str) -> Path:
        path.write_text(source.replace("{python}", sys.executable))
        path.chmod(path.stat().st_mode | stat.S_IXUSR)
        return path

    def run_main(self, *extra: str, env: dict[str, str] | None = None) -> tuple[int, str]:
        argv = [str(self.rom), "--frames", "60", "120", "--out-dir", str(self.out)]
        argv += ["--mesen2-bin", str(self.mesen2), "--neser-bin", str(self.neser)]
        argv += ["--mesen2-home", str(self.home), *extra]
        saved = dict(os.environ)
        os.environ.update({**self.env, **(env or {})})
        stdout = io.StringIO()
        try:
            with contextlib.redirect_stdout(stdout):
                code = main(argv)
        finally:
            os.environ.clear()
            os.environ.update(saved)
        return code, stdout.getvalue()

    def logged(self, program: str) -> list[str]:
        return [line for line in self.log.read_text().splitlines() if line.startswith(program + " ")]

    def test_each_mesen2_invocation_gets_its_own_rom_and_save_is_removed(self) -> None:
        self.run_main()
        runs = self.logged("mesen2")
        self.assertEqual(len(runs), 2)
        roms = [next(a for a in run.split()[1:] if a.endswith(".nes")) for run in runs]
        self.assertEqual(len({Path(r).name for r in roms}), 2, roms)
        self.assertTrue(all(Path(r).name != "kirby.nes" for r in roms), roms)
        # The second run started on no save from the first, and only the user's own save is left.
        befores = [line for line in self.log.read_text().splitlines() if line.startswith("saves-before")]
        self.assertEqual(befores, ["saves-before other-game.sav"] * 2)
        self.assertEqual(sorted(p.name for p in (self.home / "Saves").iterdir()), ["other-game.sav"])

    def test_neser_runs_on_its_own_copy_with_an_empty_config(self) -> None:
        self.run_main()
        runs = self.logged("neser")
        self.assertEqual(len(runs), 2)
        for run in runs:
            args = run.split()[1:]
            self.assertIn("--headless", args)
            self.assertEqual(args[args.index("--nes-palette") + 1], "mesen")
            config = Path(args[args.index("--config") + 1])
            self.assertNotEqual(Path(args[-1]).name, "kirby.nes")
            self.assertEqual(config.read_text() if config.exists() else "", "")

    def test_mesen2_gets_the_system_flags_and_capture_script(self) -> None:
        self.run_main("--no-game-database")
        args = self.logged("mesen2")[0].split()[1:]
        for flag in [*NES_MESEN2_FLAGS, "--testRunner", "--enableStdout", "--nes.DisableGameDatabase=true"]:
            self.assertIn(flag, args)
        self.assertTrue(args[-1].endswith("mesen2_capture.lua"))

    def test_report_prints_both_emulators_decisions(self) -> None:
        _, out = self.run_main()
        self.assertIn("Mesen2: [iNes] Mapper: 4 Sub: 0", out)
        self.assertIn("Mesen2: [DB] Mapper: 118 Sub: 0", out)
        self.assertIn("NESER:  Loaded rom with CRC32: 12345678, mapper=4", out)
        self.assertIn("NESER:  Hardware: NES (PAL)", out)
        self.assertNotIn("[DB] Initialized", out)
        # The cartridge report is printed once, not once per frame.
        self.assertEqual(out.count("[iNes] Mapper"), 1)

    def test_identical_frames_exit_zero_and_keep_the_pngs(self) -> None:
        code, out = self.run_main()
        self.assertEqual(code, 0)
        self.assertIn("frame 60: 0 differing pixels", out)
        self.assertIn("frame 120: 0 differing pixels", out)
        for name in ("neser-60.png", "mesen2-60.png", "neser-120.png", "mesen2-120.png"):
            self.assertTrue((self.out / name).is_file(), name)

    def test_differing_frames_exit_one(self) -> None:
        code, out = self.run_main(env={"FAKE_NESER_DIFFERS": "1"})
        self.assertEqual(code, 1)
        self.assertIn("frame 60: 1 differing pixels", out)

    def test_lua_error_is_reported(self) -> None:
        code, out = self.run_main(env={"FAKE_LUA_ERROR": "1"})
        self.assertEqual(code, 2)
        self.assertIn("AllowIoOsAccess", out)

    def test_failed_capture_into_a_reused_out_dir_is_not_a_match(self) -> None:
        # A rerun must not diff the previous run's PNG (nr-ocx review).
        self.assertEqual(self.run_main()[0], 0)
        code, out = self.run_main(env={"FAKE_LUA_ERROR": "1"})
        self.assertEqual(code, 2)
        self.assertNotIn("differing pixels", out)

    def test_missing_binary_is_a_capture_failure(self) -> None:
        for flag in ("--mesen2-bin", "--neser-bin"):
            with self.subTest(flag=flag):
                code, out = self.run_main(flag, str(Path(self.tmp.name) / "nonexistent"))
                self.assertEqual(code, 2)
                self.assertNotIn("differing pixels", out)

    def test_extra_mesen2_arg_replaces_a_pinned_port(self) -> None:
        self.run_main("--mesen2-arg=--nes.port2.type=Zapper")
        args = self.logged("mesen2")[0].split()[1:]
        self.assertIn("--nes.port2.type=Zapper", args)
        self.assertNotIn("--nes.port2.type=NesController", args)


if __name__ == "__main__":
    unittest.main()
