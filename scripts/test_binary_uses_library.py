"""Regression tests for the neser binary being a client of the library crate.

src/main.rs once re-declared every library module (`mod nes;` and the rest)
instead of importing them from `neser`, so each clippy and build compiled the
whole emulator a second time as the binary crate, and a crate-wide
`#![allow(dead_code)]` hid the warnings the duplicate produced (nr-xh7).
"""

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).parents[1]
MAIN_RS = ROOT / "src/main.rs"
LIBRARY_MODULES = ("frontends", "gb", "gba", "nes", "platform", "snes")


class BinaryUsesLibraryTests(unittest.TestCase):
    """main.rs imports the emulator from the library rather than compiling it again."""

    def setUp(self) -> None:
        self.source = MAIN_RS.read_text(encoding="utf-8")

    def test_main_declares_no_library_modules(self) -> None:
        """No `mod <library module>;` line, which would compile that module twice."""
        pattern = re.compile(
            r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(" + "|".join(LIBRARY_MODULES) + r")\s*;",
            re.MULTILINE,
        )
        self.assertEqual(pattern.findall(self.source), [])

    def test_main_has_no_crate_wide_dead_code_allow(self) -> None:
        """Dead code in the binary is reported, not silenced for the whole crate."""
        self.assertNotRegex(self.source, r"#!\[\s*allow\([^)]*\bdead_code\b")


if __name__ == "__main__":
    unittest.main()
