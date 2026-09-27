"""Regression tests for the install step every prepared worktree runs.

CLAUDE.md says `core.hooksPath` points at `.githooks`, whose pre-commit hook
auto-formats staged Rust. Twice a prepared worktree ran without it and the
first gate failed on `cargo fmt --check` (nr-6e9, nr-hab.1). The fleet runs
`install_shell` from `.cerebro/project.conf` in every new worktree, so that is
where the setting is re-asserted; these tests run it and check the result.
"""

import os
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


# Git isolated from the developer's own configuration (commit signing, global hooks) and from any
# GIT_* variables of an enclosing git process, so only the install step can set what is checked.
GIT_ENV = {
    **{name: value for name, value in os.environ.items() if not name.startswith("GIT_")},
    "GIT_CONFIG_GLOBAL": os.devnull,
    "GIT_CONFIG_NOSYSTEM": "1",
}


def _git(cwd: Path, *args: str) -> None:
    subprocess.run(["git", *args], cwd=cwd, env=GIT_ENV, check=True, capture_output=True)


def _hooks_path(tree: Path) -> str:
    """The tree's own core.hooksPath, or "" when it is unset."""

    return subprocess.run(
        ["git", "config", "--local", "core.hooksPath"], cwd=tree, env=GIT_ENV, capture_output=True, text=True
    ).stdout.strip()


class WorktreeInstallTests(unittest.TestCase):
    """The declared install step leaves the commit hooks active in a new worktree."""

    def test_install_shell_is_declared(self) -> None:
        """The install step is declared, so the parse below cannot silently run nothing."""

        self.assertTrue(_declared("install_shell"))

    def test_install_sets_hooks_path(self) -> None:
        """install_shell sets core.hooksPath to .githooks in a fresh worktree, before any later step can fail."""

        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            repo = base / "repo"
            repo.mkdir()
            _git(repo, "init", "-q")
            _git(repo, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "i")
            tree = base / "tree"
            _git(repo, "worktree", "add", "-q", "-b", "bead", str(tree))
            self.assertEqual(_hooks_path(tree), "", "a fresh repository must start without core.hooksPath")

            # The temporary tree has no package.json or setup-venv.sh, so the later install steps
            # fail here; the hooks path is set first so it holds even then, which is what is pinned.
            subprocess.run(
                ["bash", "-euo", "pipefail", "-c", _declared("install_shell")],
                cwd=tree,
                env=GIT_ENV,
                capture_output=True,
            )
            self.assertEqual(_hooks_path(tree), ".githooks")


if __name__ == "__main__":
    unittest.main()
