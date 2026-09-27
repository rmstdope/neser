"""Regression tests for the `rust_paths` declaration in `.cerebro/project.conf`.

`.cerebro/cerebro/scripts/build-workload --classify` fails closed when `rust_paths` is not
declared, so every producer used to classify its diff by hand (retrospectives nr-273, nr-a0g).
These tests run the real classifier against the declared pattern: every path Cargo compiles or
that only a cargo test can prove is `rust`, and everything else is `non-rust`.
"""

import os
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).parents[1]
PROJECT_CONF = ROOT / ".cerebro/project.conf"
BUILD_WORKLOAD = ROOT / ".cerebro/cerebro/scripts/build-workload"


def _declared(key: str) -> str:
    """The value of `key` in project.conf: `key value`, `#` to end of line is a comment."""

    for line in PROJECT_CONF.read_text(encoding="utf-8").splitlines():
        fields = line.split("#", 1)[0].split(None, 1)
        if len(fields) == 2 and fields[0] == key:
            return fields[1].strip()
    return ""


# project-conf reads the *shared* root's declaration, which in a fleet worktree is the main
# checkout's, not this tree's. The launcher's root hints (scripts/root-hints.sh) name the root
# explicitly and are validated against the mount, so pointing them here makes the real classifier
# read this checkout's project.conf wherever the suite runs.
CLASSIFY_ENV = {
    **os.environ,
    "CEREBRO_CONSUMER_ROOT": str(ROOT),
    "CEREBRO_CONSUMER_SHARED_ROOT": str(ROOT),
    "CEREBRO_CONSUMER_MOUNT": ".cerebro/cerebro",
}


def _classify(*paths: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(BUILD_WORKLOAD), "--classify", *paths],
        cwd=ROOT,
        env=CLASSIFY_ENV,
        capture_output=True,
        text=True,
        check=False,
    )


CARGO_INPUTS = [
    "src/nes/cpu.rs",
    "src/nes/console/testdata/savestate_golden_v8.json.gz",
    "Cargo.toml",
    "Cargo.lock",
    "build.rs",
    "rust-toolchain.toml",
    ".cargo/config.toml",
    "assets/fonts/NunitoSans-Bold.ttf",
    "roms/games/mappers/x.nes",
    "roms/automated_tests/mapper_verification/Makefile",
]

NOT_CARGO_INPUTS = [
    "web/src/main.ts",
    "web/src/x.rs",
    "scripts/gate-full.sh",
    "docs/retrospectives/nr-273.md",
    "README.md",
    "shaders/stock.slangp",
    "vendor/slang-shaders",
    "package.json",
    ".cerebro/project.conf",
    "Cargo.toml.bak",
    "docs/build.rs",
]


@unittest.skipUnless(BUILD_WORKLOAD.exists(), "the cerebro submodule is not initialised")
class RustPathsTests(unittest.TestCase):
    """build-workload classifies this tree's paths from the declared rust_paths."""

    def test_rust_paths_is_declared(self) -> None:
        self.assertTrue(_declared("rust_paths"))

    def test_cargo_inputs_classify_as_rust(self) -> None:
        for path in CARGO_INPUTS:
            with self.subTest(path=path):
                result = _classify(path)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout.strip(), "rust")

    def test_non_cargo_paths_classify_as_non_rust(self) -> None:
        for path in NOT_CARGO_INPUTS:
            with self.subTest(path=path):
                result = _classify(path)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout.strip(), "non-rust")

    def test_mixed_diff_is_rust(self) -> None:
        result = _classify("web/src/main.ts", "docs/x.md", "src/lib.rs")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "rust")


if __name__ == "__main__":
    unittest.main()
