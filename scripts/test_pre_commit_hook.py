"""Regression tests for how `.githooks/pre-commit` finds the Python formatter.

Fleet sessions never activate the venv, so a hook that looked for `ruff` on PATH only skipped
formatting in every prepared worktree, and the gate's `ruff format` leg went red later (nr-a0g,
nr-i6h). The hook now uses the worktree's `.venv/bin/ruff` first, `ruff` on PATH second, and
refuses the commit when Python is staged and neither exists (nr-jpq). These tests run the real
hook in a throwaway repository with fake formatters that record which one ran.
"""

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).parents[1]
HOOK = ROOT / ".githooks/pre-commit"

# Git isolated from the developer's own configuration and from any GIT_* variables of an
# enclosing git process (this suite may itself run inside a hook).
GIT_ENV = {
    **{name: value for name, value in os.environ.items() if not name.startswith("GIT_")},
    "GIT_CONFIG_GLOBAL": os.devnull,
    "GIT_CONFIG_NOSYSTEM": "1",
}


def _fake_ruff(path: Path, name: str) -> None:
    """A `ruff` that appends `name` to $RUFF_MARKER and succeeds."""

    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(f'#!/bin/sh\necho {name} >> "$RUFF_MARKER"\n', encoding="utf-8")
    path.chmod(0o755)


class PreCommitRuffTests(unittest.TestCase):
    """The pre-commit hook resolves ruff from the worktree's venv before PATH."""

    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        base = Path(self._tmp.name)
        self.tree = base / "tree"
        self.path_bin = base / "bin"
        self.marker = base / "marker"
        self.tree.mkdir()
        self.path_bin.mkdir()
        subprocess.run(["git", "init", "-q"], cwd=self.tree, env=GIT_ENV, check=True)
        hook = self.tree / ".githooks/pre-commit"
        hook.parent.mkdir()
        shutil.copy(HOOK, hook)

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def _stage(self, name: str) -> None:
        (self.tree / name).write_text("x = 1\n", encoding="utf-8")
        subprocess.run(["git", "add", name], cwd=self.tree, env=GIT_ENV, check=True)

    def _run_hook(self) -> subprocess.CompletedProcess[str]:
        env = {**GIT_ENV, "PATH": f"{self.path_bin}:/usr/bin:/bin", "RUFF_MARKER": str(self.marker)}
        return subprocess.run(["sh", ".githooks/pre-commit"], cwd=self.tree, env=env, capture_output=True, text=True)

    def _ran(self) -> list[str]:
        return self.marker.read_text(encoding="utf-8").split() if self.marker.exists() else []

    def test_prefers_the_worktree_venv_ruff(self) -> None:
        """With both a venv ruff and one on PATH, only the venv one formats."""

        _fake_ruff(self.tree / ".venv/bin/ruff", "venv")
        _fake_ruff(self.path_bin / "ruff", "path")
        self._stage("a.py")
        result = self._run_hook()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self._ran(), ["venv"])

    def test_falls_back_to_ruff_on_path(self) -> None:
        """Without a venv ruff, the one on PATH formats."""

        _fake_ruff(self.path_bin / "ruff", "path")
        self._stage("a.py")
        result = self._run_hook()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self._ran(), ["path"])

    def test_fails_when_python_is_staged_and_no_ruff_exists(self) -> None:
        """Staged Python with no ruff anywhere refuses the commit and says where it looked."""

        self._stage("a.py")
        result = self._run_hook()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(".venv/bin/ruff", result.stdout + result.stderr)

    def test_no_python_staged_needs_no_ruff(self) -> None:
        """A commit without Python goes through even with no ruff installed."""

        self._stage("notes.txt")
        result = self._run_hook()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self._ran(), [])


if __name__ == "__main__":
    unittest.main()
