"""The local gate's Rust unit tests compile the feature set CI compiles (nr-5ku).

CI builds its unit-test archive with ``cargo nextest archive --lib <flags>``. The local gate ran
``cargo test --no-default-features --lib`` instead, so ``src/frontends/native`` was never compiled
before a pull request and a desktop regression turned red only in CI. These tests tie the local
legs to whatever flags CI's archive uses, so the two cannot drift apart again.
"""

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CI_YML = ROOT / ".github" / "workflows" / "ci.yml"
GATE_FULL = ROOT / "scripts" / "gate-full.sh"
TEST_DIR = ROOT / "scripts" / "test-dir.sh"


def ci_archive_flags() -> list[str]:
    """The feature flags on CI's ``cargo nextest archive --lib`` line."""

    match = re.search(r"cargo nextest archive (.*)$", CI_YML.read_text(encoding="utf-8"), re.MULTILINE)
    if match is None:
        return []
    return feature_flags(match.group(1).split())


def feature_flags(words: list[str]) -> list[str]:
    """The feature-selecting words of a cargo command line, in order."""

    flags: list[str] = []
    for i, word in enumerate(words):
        if word in ("--all-features", "--no-default-features"):
            flags.append(word)
        elif word == "--features" and i + 1 < len(words):
            flags.extend([word, words[i + 1]])
        elif word.startswith("--features="):
            flags.append(word)
    return flags


def gate_unit_test_lines() -> list[str]:
    """Every ``cargo test`` step in gate-full.sh that runs the lib's unit tests."""

    lines = GATE_FULL.read_text(encoding="utf-8").splitlines()
    return [line for line in lines if re.match(r"\s*step cargo test\b", line) and "--lib" in line.split()]


class GateCompilesWhatCiCompilesTest(unittest.TestCase):
    def test_ci_archive_flags_found(self) -> None:
        """Guards the other tests: an unparsed CI line would make them pass vacuously."""

        self.assertIn("--lib", CI_YML.read_text(encoding="utf-8"))
        self.assertNotEqual(ci_archive_flags(), [])

    def test_gate_unit_legs_use_ci_features(self) -> None:
        lines = gate_unit_test_lines()
        self.assertGreaterEqual(len(lines), 2, "expected the fast and the full unit-test legs")
        for line in lines:
            with self.subTest(line=line.strip()):
                self.assertEqual(feature_flags(line.split()), ci_archive_flags())

    def test_test_dir_default_uses_ci_features(self) -> None:
        match = re.search(r'CARGO_FLAGS="\$\{CARGO_TEST_ARGS:-([^}]*)\}"', TEST_DIR.read_text(encoding="utf-8"))
        self.assertIsNotNone(match, "test-dir.sh's CARGO_TEST_ARGS default not found")
        assert match is not None
        self.assertEqual(feature_flags(match.group(1).split()), ci_archive_flags())


if __name__ == "__main__":
    unittest.main()
