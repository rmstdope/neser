"""Tests for ``ensure_playwright_deps.sh``, which keeps CI off the apt mirror (nr-2e2l).

The GitHub runner image already carries every library Chromium links against, so the script only
falls back to ``npx playwright install-deps`` when ``ldd`` reports one missing. ``ldd`` and ``npx``
are faked on ``PATH``; each fake records its calls in a log file.
"""

import stat
import subprocess
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "ensure_playwright_deps.sh"


def _executable(path: Path, body: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("#!/bin/sh\n" + body)
    path.chmod(path.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)


class EnsurePlaywrightDepsTest(unittest.TestCase):
    """Given what the installed Chromium links against, the script decides whether to touch apt."""

    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = Path(self._tmp.name)
        self.browsers = self.tmp / "ms-playwright"
        self.browsers.mkdir()
        self.bin = self.tmp / "bin"
        self.npx_log = self.tmp / "npx.log"
        self.missing_for = ""
        self.npx_status = 0
        self.ldd_body = ""

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def _install_headless_shell(self) -> Path:
        shell_dir = self.browsers / "chromium_headless_shell-1248" / "chrome-headless-shell-linux64"
        shell_dir.mkdir(parents=True)
        binary = shell_dir / "chrome-headless-shell"
        binary.write_text("")
        (shell_dir / "libEGL.so").write_text("")
        return binary

    def _run(self) -> subprocess.CompletedProcess[str]:
        # The fake ldd reports a library missing for any file whose name contains missing_for.
        missing = self.missing_for or "\x01never\x01"
        _executable(
            self.bin / "ldd",
            self.ldd_body
            or (
                'for f in "$@"; do\n'
                '  echo "$f:"\n'
                f'  case "$f" in *"{missing}"*) echo "\tlibfoo.so.1 => not found" ;; '
                '*) echo "\tlibc.so.6 => /lib/libc.so.6" ;; esac\n'
                "done\n"
            ),
        )
        _executable(self.bin / "npx", f'echo "$*" >> "{self.npx_log}"\nexit {self.npx_status}\n')
        env = {
            "PATH": f"{self.bin}:/usr/bin:/bin",
            "HOME": str(self.tmp),
            "PLAYWRIGHT_BROWSERS_PATH": str(self.browsers),
            "PLAYWRIGHT_DEPS_RETRY_DELAY": "0",
        }
        return subprocess.run(["sh", str(SCRIPT)], env=env, capture_output=True, text=True, check=False)

    def _npx_calls(self) -> list[str]:
        return self.npx_log.read_text().splitlines() if self.npx_log.exists() else []

    def test_all_libraries_present_skips_apt(self) -> None:
        self._install_headless_shell()
        result = self._run()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self._npx_calls(), [])
        self.assertIn("skipping apt", result.stdout)

    def test_missing_library_runs_install_deps(self) -> None:
        self._install_headless_shell()
        self.missing_for = "chrome-headless-shell"
        result = self._run()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self._npx_calls(), ["playwright install-deps chromium"])
        self.assertIn("libfoo.so.1", result.stdout)

    def test_shared_libraries_beside_the_binary_are_checked(self) -> None:
        self._install_headless_shell()
        self.missing_for = "libEGL.so"
        result = self._run()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self._npx_calls(), ["playwright install-deps chromium"])

    def test_bundled_libraries_resolve_from_their_own_directory(self) -> None:
        """A bundled library naming a sibling by bare soname finds it, as Chromium itself does.

        Playwright's own check puts the browser directory on LD_LIBRARY_PATH; without it, ldd
        would report the sibling missing on every runner and send every run to apt (review of
        PR #3368, finding 1).
        """

        shell_dir = self._install_headless_shell().parent
        self.ldd_body = (
            'for f in "$@"; do\n'
            '  echo "$f:"\n'
            f'  case ":${{LD_LIBRARY_PATH:-}}:" in *":{shell_dir}:"*) echo "\tlibEGL.so => {shell_dir}/libEGL.so" ;; '
            '*) echo "\tlibEGL.so => not found" ;; esac\n'
            "done\n"
        )
        result = self._run()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self._npx_calls(), [])

    def test_no_browser_installed_runs_install_deps(self) -> None:
        result = self._run()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self._npx_calls(), ["playwright install-deps chromium"])

    def test_install_deps_failure_fails_after_three_attempts(self) -> None:
        self._install_headless_shell()
        self.missing_for = "chrome-headless-shell"
        self.npx_status = 1
        result = self._run()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self._npx_calls(), ["playwright install-deps chromium"] * 3)


if __name__ == "__main__":
    unittest.main()
