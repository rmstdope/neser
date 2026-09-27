"""build.rs never asks Cargo to watch a path that does not exist (nr-5ku).

Cargo treats a missing ``rerun-if-changed`` path as changed on every build. ``roms/games`` is
gitignored and absent from most checkouts and every fleet worktree, so watching
``roms/games/mappers`` rebuilt the whole crate on every cargo call: the gate's two unit-test legs
compiled it twice, and a ``test-dir.sh`` that lists a directory's tests before running them would
have compiled it once per directory.

The test compiles build.rs on its own with the host ``rustc`` and runs it in an empty directory.
"""

import os
import re
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BUILD_RS = ROOT / "build.rs"


ENV = {k: v for k, v in os.environ.items() if k != "RUSTUP_TOOLCHAIN"}


def compile_build_script(out_dir: Path) -> Path:
    exe = out_dir / "build-script"
    subprocess.run(
        ["rustc", "--edition", "2024", "-o", str(exe), str(BUILD_RS)],
        cwd=out_dir,
        env=ENV,
        check=True,
        capture_output=True,
    )
    return exe


def run_build_script(exe: Path, cwd: Path) -> list[str]:
    """The ``rerun-if-changed`` paths build.rs prints when run in ``cwd``."""

    out = subprocess.run(
        [str(exe)], cwd=cwd, env=dict(ENV, OUT_DIR=str(cwd)), check=True, capture_output=True, text=True
    ).stdout
    prefix = "cargo:rerun-if-changed="
    return [line[len(prefix) :] for line in out.splitlines() if line.startswith(prefix)]


@unittest.skipIf(shutil.which("rustc") is None, "needs rustc")
class BuildScriptRerunHintsTest(unittest.TestCase):
    exe_dir: tempfile.TemporaryDirectory[str]
    exe: Path

    @classmethod
    def setUpClass(cls) -> None:
        cls.exe_dir = tempfile.TemporaryDirectory()
        cls.exe = compile_build_script(Path(cls.exe_dir.name))

    @classmethod
    def tearDownClass(cls) -> None:
        cls.exe_dir.cleanup()

    def test_no_hint_names_a_missing_path(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            cwd = Path(tmp)
            (cwd / "src/gba/bios").mkdir(parents=True)
            (cwd / "src/gba/bios/bios.bin").write_bytes(b"")
            hints = run_build_script(self.exe, cwd)
            for hint in hints:
                with self.subTest(hint=hint):
                    self.assertTrue((cwd / hint).exists(), f"build.rs watches missing {hint}")

    def test_existing_mapper_games_are_watched(self) -> None:
        """Adding or changing a .autorun file must still regenerate the autorun tests."""

        with tempfile.TemporaryDirectory() as tmp:
            cwd = Path(tmp)
            games = cwd / "roms/games/mappers/mmc1"
            games.mkdir(parents=True)
            (games / "a.autorun").write_text("", encoding="utf-8")
            hints = run_build_script(self.exe, cwd)
            self.assertIn("roms/games/mappers", hints)
            self.assertIn("roms/games/mappers/mmc1/a.autorun", hints)


class CiRunsTheBuildScriptTestsTest(unittest.TestCase):
    """A PR touching only build.rs runs this file's tests and the Rust suite in CI."""

    def test_python_and_rust_filters_include_build_rs(self) -> None:
        ci = (ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")
        for name in ("python", "rust"):
            with self.subTest(filter=name):
                block = re.search(rf"^ {{12}}{name}:\n((?: {{14}}.*\n)+)", ci, re.MULTILINE)
                self.assertIsNotNone(block, f"no {name} path filter in ci.yml")
                assert block is not None
                self.assertIn("- 'build.rs'", block.group(1))


if __name__ == "__main__":
    unittest.main()
