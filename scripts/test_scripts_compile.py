"""Guard test: every Python module under ``scripts/`` must be syntactically valid.

Test discovery only imports files named ``test_*.py``, so a module that no test
imports -- an application entry point, for example -- can rot into a SyntaxError
without any check noticing. ``scripts/mappertool/main.py`` did exactly that and
went undetected. Byte-compiling every module closes that blind spot.
"""

import py_compile
import tempfile
import unittest
from pathlib import Path

SCRIPTS_ROOT = Path(__file__).resolve().parent


def _python_sources() -> list[Path]:
    """Return every Python source under scripts/, excluding bytecode caches."""
    return sorted(p for p in SCRIPTS_ROOT.rglob("*.py") if "__pycache__" not in p.parts)


class TestScriptsCompile(unittest.TestCase):
    """Given the Python tools, when each module is compiled, then none has a syntax error."""

    def test_every_python_module_under_scripts_compiles(self) -> None:
        sources = _python_sources()
        self.assertGreater(len(sources), 0, "no Python sources found under scripts/")

        failures: list[str] = []
        with tempfile.TemporaryDirectory() as cache_dir:
            for source in sources:
                relative = source.relative_to(SCRIPTS_ROOT)
                cfile = Path(cache_dir) / (relative.as_posix().replace("/", "_") + "c")
                try:
                    py_compile.compile(str(source), cfile=str(cfile), doraise=True)
                except py_compile.PyCompileError as exc:
                    failures.append(f"{relative}: {exc.msg.strip()}")

        self.assertEqual(
            [],
            failures,
            "Python modules under scripts/ failed to compile:\n" + "\n".join(failures),
        )


if __name__ == "__main__":
    unittest.main()
