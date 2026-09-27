"""Tests for scripts/chromedriver_match.py, which picks a ChromeDriver matching the installed Chrome."""

import contextlib
import io
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

from scripts.chromedriver_match import (
    choose_driver,
    default_chrome_paths,
    default_driver_paths,
    main,
    parse_version,
)

SCRIPT = Path(__file__).resolve().parent / "chromedriver_match.py"
SYSTEM_PYTHON = Path("/usr/bin/python3")


def _fake_binary(directory: Path, name: str, output: str) -> Path:
    """Write an executable shell script that prints `output` and return its path."""
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / name
    path.write_text(f"#!/bin/sh\necho '{output}'\n")
    path.chmod(0o755)
    return path


class TestParseVersion(unittest.TestCase):
    """Given a browser or driver's --version output, when parsed, then the dotted version comes back."""

    def test_parse_version_reads_chrome_and_chromedriver_output(self) -> None:
        self.assertEqual((153, 0, 8010, 53), parse_version("Google Chrome 153.0.8010.53 \n"))
        self.assertEqual(
            (152, 0, 7977, 64),
            parse_version(
                "ChromeDriver 152.0.7977.64 (506c834ecceaa943c5f41e6cfe7f68acb5c45346-refs/branch-heads/7977@{#1891})"
            ),
        )
        self.assertEqual((120, 0, 6099, 71), parse_version("Chromium 120.0.6099.71 built on Debian"))

    def test_parse_version_returns_none_without_a_version(self) -> None:
        self.assertIsNone(parse_version("command not found"))


class TestChooseDriver(unittest.TestCase):
    """Given Chrome's version and candidate drivers, when choosing, then only a matching major is taken."""

    def test_choose_driver_prefers_first_matching_major(self) -> None:
        drivers = [
            (Path("/cache/a/chromedriver"), (152, 0, 7977, 64)),
            (Path("/path/chromedriver"), (153, 0, 8010, 1)),
            (Path("/cache/b/chromedriver"), (153, 0, 8010, 53)),
        ]
        self.assertEqual(Path("/path/chromedriver"), choose_driver((153, 0, 8010, 53), drivers))

    def test_choose_driver_returns_none_when_no_major_matches(self) -> None:
        drivers = [
            (Path("/cache/a/chromedriver"), (152, 0, 7977, 64)),
            (Path("/cache/b/chromedriver"), (154, 0, 8037, 57)),
            (Path("/broken/chromedriver"), None),
        ]
        self.assertIsNone(choose_driver((153, 0, 8010, 53), drivers))


class TestMain(unittest.TestCase):
    """Given a Chrome and candidate drivers, when the script runs, then it prints a match or stops in one line."""

    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        self.chrome = _fake_binary(self.root / "chrome", "chrome", "Google Chrome 153.0.8010.53 ")
        self.d152 = _fake_binary(self.root / "a", "chromedriver", "ChromeDriver 152.0.7977.64 (abc)")
        self.d153 = _fake_binary(self.root / "b", "chromedriver", "ChromeDriver 153.0.8010.53 (def)")
        self.d154 = _fake_binary(self.root / "c", "chromedriver", "ChromeDriver 154.0.8037.57 (ghi)")

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def _run(self, argv: list[str]) -> tuple[int, str, str]:
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = main(argv)
        return code, out.getvalue(), err.getvalue()

    def test_main_prints_matching_driver_path(self) -> None:
        argv = ["--chrome", str(self.chrome), "--driver", str(self.d152), "--driver", str(self.d153)]
        code, out, err = self._run(argv)
        self.assertEqual(0, code)
        self.assertEqual(f"{self.d153}\n", out)
        self.assertEqual("", err)

    def test_main_stops_with_one_line_naming_the_mismatch(self) -> None:
        argv = ["--chrome", str(self.chrome), "--driver", str(self.d152), "--driver", str(self.d154)]
        code, out, err = self._run(argv)
        self.assertEqual(1, code)
        self.assertEqual("", out)
        self.assertEqual(1, err.count("\n"), err)
        self.assertTrue(err.endswith("\n"))
        for part in ("153.0.8010.53", "152.0.7977.64", str(self.d152), "154.0.8037.57", str(self.d154)):
            self.assertIn(part, err)
        self.assertIn("put a ChromeDriver 153 first on PATH", err)

    def test_main_stops_with_one_line_when_no_driver_is_found(self) -> None:
        code, _out, err = self._run(["--chrome", str(self.chrome), "--driver", str(self.root / "missing")])
        self.assertEqual(1, code)
        self.assertEqual(1, err.count("\n"), err)
        self.assertIn("found: none", err)

    def test_main_names_a_driver_that_cannot_run_instead_of_saying_none(self) -> None:
        broken = self.root / "d" / "chromedriver"
        broken.parent.mkdir()
        broken.write_text("not a program")
        code, _out, err = self._run(["--chrome", str(self.chrome), "--driver", str(broken)])
        self.assertEqual(1, code)
        self.assertEqual(1, err.count("\n"), err)
        self.assertIn(f"unreadable at {broken}", err)
        self.assertNotIn("found: none", err)

    def test_main_stops_when_chrome_is_missing(self) -> None:
        code, out, err = self._run(["--chrome", str(self.root / "no-chrome"), "--driver", str(self.d153)])
        self.assertEqual(1, code)
        self.assertEqual("", out)
        self.assertEqual(1, err.count("\n"), err)
        self.assertIn("Chrome", err)


class TestDefaultDriverPaths(unittest.TestCase):
    """Given drivers on PATH and in wasm-pack's cache, when listed, then PATH comes first."""

    def test_default_driver_paths_put_path_before_wasm_pack_cache(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            home = root / "home"
            mac_cache = _fake_binary(home / "Library/Caches/.wasm-pack/chromedriver-1", "chromedriver", "x")
            linux_cache = _fake_binary(home / ".cache/.wasm-pack/chromedriver-2", "chromedriver", "x")
            on_path = _fake_binary(root / "bin", "chromedriver", "x")
            env = {"PATH": os.pathsep.join([str(root / "empty"), str(root / "bin")])}
            self.assertEqual([on_path, mac_cache, linux_cache], default_driver_paths(env, home))


class TestDefaultChromePaths(unittest.TestCase):
    """Given Chrome in the user's Applications folder and on PATH, when listed, then both are found."""

    def test_default_chrome_paths_find_the_user_app_bundle_and_path_browsers(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            home = root / "home"
            user_app = _fake_binary(home / "Applications/Google Chrome.app/Contents/MacOS", "Google Chrome", "x")
            linux = _fake_binary(root / "bin", "google-chrome", "x")
            paths = default_chrome_paths({"PATH": str(root / "bin")}, home)
            self.assertIn(user_app, paths)
            self.assertIn(linux, paths)
            self.assertLess(paths.index(user_app), paths.index(linux))


class TestSystemPython(unittest.TestCase):
    """Given the system python3 the gate falls back to, when the script runs, then it does not crash on import."""

    @unittest.skipUnless(SYSTEM_PYTHON.is_file(), "no /usr/bin/python3")
    def test_script_runs_under_the_system_python(self) -> None:
        result = subprocess.run(
            [str(SYSTEM_PYTHON), str(SCRIPT), "--chrome", "/nonexistent/chrome"],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(1, result.returncode, result.stderr)
        self.assertNotIn("Traceback", result.stderr)
        self.assertIn("Chrome not found", result.stderr)


if __name__ == "__main__":
    unittest.main()
