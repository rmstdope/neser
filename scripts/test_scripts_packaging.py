"""Guard tests: every tool directory under ``scripts/`` is a real Python package.

Two of the tool directories were historically named with hyphens, which are not
valid Python identifiers. That forced two different import workarounds -- a
``sys.path`` mutation in one and a dead ``try``/``except ImportError`` dance in
the other -- and forced test discovery to run three times with separate roots.

Requiring real packages keeps imports ordinary, lets a single discovery run find
every test, and gives type checkers resolvable dotted module paths.
"""

import importlib
import unittest
from pathlib import Path

SCRIPTS_ROOT = Path(__file__).resolve().parent

TOOL_PACKAGES = [
    "scripts.gb_boot_rom",
    "scripts.mappertool",
    "scripts.metadata_scraper",
    "scripts.nes_rom_db_scraper",
]

# The shared data-access modules that carry type annotations and are checked
# strictly. They must be reachable by dotted path for mypy overrides to apply.
ANNOTATED_MODULES = [
    "scripts.metadata_scraper.api_client",
    "scripts.metadata_scraper.metadata_db",
    "scripts.nes_rom_db_scraper.rom_database",
]


def _tool_directories() -> list[Path]:
    """Return each immediate subdirectory of scripts/ that holds Python sources."""
    return sorted(d for d in SCRIPTS_ROOT.iterdir() if d.is_dir() and d.name != "__pycache__" and any(d.glob("*.py")))


class TestToolPackages(unittest.TestCase):
    """Given the Python tool directories, when imported by dotted path, then each resolves."""

    def test_every_tool_directory_name_is_a_valid_identifier(self) -> None:
        offenders = [d.name for d in _tool_directories() if not d.name.isidentifier()]
        self.assertEqual([], offenders, f"tool directories cannot be imported as packages: {offenders}")

    def test_every_tool_directory_has_an_init_module(self) -> None:
        missing = [d.name for d in _tool_directories() if not (d / "__init__.py").is_file()]
        self.assertEqual([], missing, f"tool directories missing __init__.py: {missing}")

    def test_every_tool_package_is_importable(self) -> None:
        for name in TOOL_PACKAGES:
            with self.subTest(package=name):
                importlib.import_module(name)

    def test_annotated_modules_are_importable_by_dotted_path(self) -> None:
        for name in ANNOTATED_MODULES:
            with self.subTest(module=name):
                importlib.import_module(name)


if __name__ == "__main__":
    unittest.main()
