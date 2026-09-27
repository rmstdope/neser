"""Tests for the single list of slow integration-test modules in ``scripts/test-dir.sh`` (nr-c3u).

Which ``<console>::integration_tests`` modules count as slow used to be written both in
``test-dir.sh --skip-integration`` and in the nextest expression of CI's unit-only fallback, and
the two drifted for 85 days. The script now owns the list and CI asks it for the expression.
"""

import os
import re
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TEST_DIR = ROOT / "scripts" / "test-dir.sh"
CI_YML = ROOT / ".github" / "workflows" / "ci.yml"

SLOW_MODULES = [
    "nes::integration_tests",
    "gb::integration_tests",
    "gba::integration_tests",
    "snes::integration_tests",
]


def run_test_dir(*args: str, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(TEST_DIR), *args],
        capture_output=True,
        text=True,
        check=True,
        env=env,
    )


def printed_expression() -> str:
    return run_test_dir("--print-nextest-skip-expr").stdout.strip()


def modules_in(expression: str) -> list[str]:
    return re.findall(r"test\(([^)]*)\)", expression)


class NextestSkipExpressionTest(unittest.TestCase):
    """``--print-nextest-skip-expr`` prints the filter CI's unit-only run uses."""

    def test_prints_nextest_expression_for_every_slow_module(self) -> None:
        expected = "not (" + " | ".join(f"test({m})" for m in SLOW_MODULES) + ")"
        self.assertEqual(printed_expression(), expected)

    def test_needs_no_directory_argument(self) -> None:
        result = subprocess.run([str(TEST_DIR), "--print-nextest-skip-expr"], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)


class SkipIntegrationTest(unittest.TestCase):
    """``--skip-integration`` skips exactly the modules the expression names."""

    def test_skip_integration_passes_a_skip_per_slow_module(self) -> None:
        with tempfile.TemporaryDirectory() as fake_bin:
            cargo = Path(fake_bin) / "cargo"
            cargo.write_text('#!/bin/sh\nprintf "%s\\n" "$@"\n', encoding="utf-8")
            cargo.chmod(cargo.stat().st_mode | stat.S_IXUSR)
            env = dict(os.environ, PATH=f"{fake_bin}{os.pathsep}{os.environ['PATH']}")
            argv = run_test_dir("src/nes", "--skip-integration", env=env).stdout.splitlines()

        skipped = [argv[i + 1] for i, arg in enumerate(argv) if arg == "--skip"]
        self.assertEqual(skipped, modules_in(printed_expression()))
        self.assertEqual(skipped, SLOW_MODULES)


class CiReadsSkipListTest(unittest.TestCase):
    """CI's unit-only fallback takes its filter from the script instead of spelling it out."""

    def setUp(self) -> None:
        self.ci = CI_YML.read_text(encoding="utf-8")

    def test_unit_fallback_asks_the_script(self) -> None:
        self.assertIn('-E "$(./scripts/test-dir.sh --print-nextest-skip-expr)"', self.ci)

    def test_ci_names_no_integration_module_itself(self) -> None:
        self.assertNotRegex(self.ci, r"test\(\w+::integration_tests\)")


if __name__ == "__main__":
    unittest.main()
