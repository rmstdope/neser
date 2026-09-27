"""Regression tests for the install step every prepared worktree runs.

CLAUDE.md says `core.hooksPath` points at `.githooks`, whose pre-commit hook
auto-formats staged Rust. Twice a prepared worktree ran without it and the
first gate failed on `cargo fmt --check` (nr-6e9, nr-hab.1). The fleet runs
`install_shell` from `.cerebro/project.conf` in every new worktree, so that is
where the setting is re-asserted; these tests run it and check the result.
"""

import os
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).parents[1]
PROJECT_CONF = ROOT / ".cerebro/project.conf"


def _declared(key: str) -> str:
    """The value of `key` in project.conf: `key value`, `#` to end of line is a comment."""

    for line in PROJECT_CONF.read_text(encoding="utf-8").splitlines():
        fields = line.split("#", 1)[0].split(None, 1)
        if len(fields) == 2 and fields[0] == key:
            return fields[1].strip()
    return ""


def _git(cwd: Path, *args: str) -> str:
    return subprocess.run(
        ["git", *args], cwd=cwd, check=True, capture_output=True, text=True
    ).stdout.strip()


class WorktreeInstallTests(unittest.TestCase):
    """The declared install step leaves the commit hooks active in a new worktree."""

    def test_install_shell_is_declared(self) -> None:
        """The install step is declared, so the parse below cannot silently run nothing."""

        self.assertTrue(_declared("install_shell"))

    def test_install_sets_hooks_path(self) -> None:
        """Running install_shell in a fresh linked worktree sets core.hooksPath to .githooks."""

        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            repo = base / "repo"
            repo.mkdir()
            _git(repo, "init", "-q")
            _git(repo, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "i")
            tree = base / "tree"
            _git(repo, "worktree", "add", "-q", "-b", "bead", str(tree))
            self.assertEqual(
                subprocess.run(
                    ["git", "config", "--local", "core.hooksPath"], cwd=tree, capture_output=True
                ).returncode,
                1,
                "a fresh repository must start without core.hooksPath",
            )

            # A stub npm, so the test proves the git setting without a real `npm ci`.
            bin_dir = base / "bin"
            bin_dir.mkdir()
            npm = bin_dir / "npm"
            npm.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            npm.chmod(npm.stat().st_mode | stat.S_IXUSR)
            env = {**os.environ, "PATH": f"{bin_dir}{os.pathsep}{os.environ['PATH']}"}

            subprocess.run(
                ["bash", "-euo", "pipefail", "-c", _declared("install_shell")],
                cwd=tree,
                env=env,
                check=True,
                capture_output=True,
            )
            self.assertEqual(_git(tree, "config", "--local", "core.hooksPath"), ".githooks")


if __name__ == "__main__":
    unittest.main()
