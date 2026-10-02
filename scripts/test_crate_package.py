"""The crate ``cargo publish`` uploads fits crates.io and still carries what the build reads (nr-d2l).

v1.3.0's "Publish to crates.io" job packaged 1155 files, 38.4 MiB (10.8 MiB compressed), and
crates.io refused it with 413 Payload Too Large: its upload limit is 10 MiB. The release workflow
publishes with ``--no-verify``, so nothing caught the size before the tag. These tests package the
crate the same way and check both the size and that every file the source embeds at compile time
is still in it.
"""

import os
import re
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CRATES_IO_MAX_UPLOAD = 10 * 1024 * 1024
INCLUDE_RE = re.compile(r"include_(?:str|bytes)!\(\s*\"([^\"]+)\"\s*\)")


def cargo(*args: str, target_dir: Path) -> str:
    return subprocess.run(
        ["cargo", *args, "--allow-dirty", "--offline"],
        cwd=ROOT,
        env=dict(os.environ, CARGO_TARGET_DIR=str(target_dir)),
        check=True,
        capture_output=True,
        text=True,
    ).stdout


@unittest.skipIf(shutil.which("cargo") is None, "needs cargo")
class CratePackageTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls._tmp = tempfile.TemporaryDirectory()
        target = Path(cls._tmp.name)
        cls.listed = set(cargo("package", "--list", target_dir=target).split())
        cargo("package", "--no-verify", target_dir=target)
        cls.crates = list((target / "package").glob("neser-*.crate"))

    @classmethod
    def tearDownClass(cls) -> None:
        cls._tmp.cleanup()

    def test_packaged_crate_is_under_the_crates_io_upload_limit(self) -> None:
        self.assertEqual(len(self.crates), 1, self.crates)
        size = self.crates[0].stat().st_size
        self.assertLess(size, CRATES_IO_MAX_UPLOAD, f"{self.crates[0].name} is {size} bytes")

    def test_package_carries_every_file_the_source_embeds(self) -> None:
        missing = []
        for rs in sorted((ROOT / "src").rglob("*.rs")):
            for rel in INCLUDE_RE.findall(rs.read_text(encoding="utf-8")):
                path = (rs.parent / rel).resolve().relative_to(ROOT).as_posix()
                if path not in self.listed:
                    missing.append(f"{rs.relative_to(ROOT)} -> {path}")
        self.assertEqual(missing, [])

    def test_package_carries_the_build_script_and_default_shader(self) -> None:
        for path in ("build.rs", "src/gba/bios/bios.bin", "shaders/stock.slangp", "shaders/stock.slang"):
            self.assertIn(path, self.listed)


if __name__ == "__main__":
    unittest.main()
