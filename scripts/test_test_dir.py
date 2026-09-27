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

    def test_prints_a_nextest_negation_of_integration_modules(self) -> None:
        """The list itself lives only in test-dir.sh, so this pins its shape, not its entries."""

        expression = printed_expression()
        self.assertRegex(expression, r"^not \(test\([^)]+\)( \| test\([^)]+\))*\)$")
        modules = modules_in(expression)
        self.assertGreater(len(modules), 0)
        for module in modules:
            self.assertRegex(module, r"^\w+::integration_tests$")

    def test_needs_no_directory_argument(self) -> None:
        result = subprocess.run([str(TEST_DIR), "--print-nextest-skip-expr"], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)


# A stand-in for cargo: a ``--list`` call answers one test unless the filter is
# ``empty::``; any other call prints its arguments, one per line, and records that it ran.
FAKE_CARGO = """#!/bin/sh
case " $* " in
  *" --list "*)
    case " $* " in
      *" empty::"*) echo "0 tests, 0 benchmarks" ;;
      *) echo "some::module::a_test: test"; echo "1 test, 0 benchmarks" ;;
    esac ;;
  *)
    echo ran >> "$FAKE_CARGO_LOG"
    printf "%s\\n" "$@" ;;
esac
"""


def run_with_fake_cargo(*args: str) -> tuple[subprocess.CompletedProcess[str], bool]:
    """Runs test-dir.sh against FAKE_CARGO; returns the result and whether the test run happened."""

    with tempfile.TemporaryDirectory() as fake_bin:
        cargo = Path(fake_bin) / "cargo"
        cargo.write_text(FAKE_CARGO, encoding="utf-8")
        cargo.chmod(cargo.stat().st_mode | stat.S_IXUSR)
        log = Path(fake_bin) / "ran.log"
        env = dict(os.environ, PATH=f"{fake_bin}{os.pathsep}{os.environ['PATH']}", FAKE_CARGO_LOG=str(log))
        result = subprocess.run([str(TEST_DIR), *args], capture_output=True, text=True, env=env)
        return result, log.exists()


class SkipIntegrationTest(unittest.TestCase):
    """``--skip-integration`` skips exactly the modules the expression names."""

    def test_skip_integration_passes_a_skip_per_slow_module(self) -> None:
        result, _ = run_with_fake_cargo("src/nes", "--skip-integration")
        self.assertEqual(result.returncode, 0, result.stderr)
        argv = result.stdout.splitlines()

        skipped = [argv[i + 1] for i, arg in enumerate(argv) if arg == "--skip"]
        self.assertEqual(skipped, modules_in(printed_expression()))


class ZeroMatchTest(unittest.TestCase):
    """A requested directory that matches no test fails instead of passing as ``0 passed`` (nr-5ku)."""

    def test_directory_with_no_tests_fails(self) -> None:
        result, ran = run_with_fake_cargo("src/empty")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("src/empty", result.stderr)
        self.assertIn("matches no test", result.stderr)
        self.assertFalse(ran, "the tests must not run once a directory matched none")

    def test_one_empty_directory_among_several_fails(self) -> None:
        result, ran = run_with_fake_cargo("src/nes", "src/empty/")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("src/empty/", result.stderr)
        self.assertNotIn("src/nes ", result.stderr)
        self.assertFalse(ran)

    def test_directory_with_tests_runs(self) -> None:
        result, ran = run_with_fake_cargo("src/nes")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(ran)
        self.assertIn("nes::", result.stdout.splitlines())


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
