"""Regression tests for the gate's Python environment (bead nr-i6h).

A prepared worktree once had no `.venv`, and `py()` in `scripts/gate-full.sh`
silently fell back to the system `python3`: the unittest leg failed with import
errors, ruff and mypy were missing, and each producer rebuilt a venv by hand.
Now `scripts/setup-venv.sh` builds `.venv` from the dependency groups CI
installs, the worktree install runs it, and the full gate refuses to start
without it.
"""

import os
import re
import shutil
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).parents[1]
GATE = ROOT / "scripts/gate-full.sh"
SETUP = ROOT / "scripts/setup-venv.sh"


def _write_executable(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")
    path.chmod(path.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)


class _Sandbox(unittest.TestCase):
    """A temporary git repository with one script copied in and a fake-tool bin dir on PATH."""

    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.repo = Path(self._tmp.name) / "repo"
        self.bin = Path(self._tmp.name) / "bin"
        self.log = Path(self._tmp.name) / "calls.log"
        (self.repo / "scripts").mkdir(parents=True)
        self.bin.mkdir()
        subprocess.run(["git", "init", "-q", str(self.repo)], check=True)

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def copy_script(self, script: Path) -> Path:
        target = self.repo / "scripts" / script.name
        shutil.copy(script, target)
        return target

    def fake_tool(self, name: str, body: str = "") -> None:
        _write_executable(self.bin / name, f'#!/usr/bin/env bash\necho "{name} $*" >> "{self.log}"\n{body}\n')

    def run_script(self, script: Path, *args: str) -> subprocess.CompletedProcess[str]:
        env = {**os.environ, "PATH": f"{self.bin}{os.pathsep}{os.environ['PATH']}"}
        env.pop("PYTHON", None)
        return subprocess.run(
            ["bash", str(script), *args], cwd=self.repo, env=env, capture_output=True, text=True, check=False
        )

    def calls(self) -> list[str]:
        return self.log.read_text(encoding="utf-8").splitlines() if self.log.exists() else []


class GateWithoutVenvTests(_Sandbox):
    """The full gate stops at once, with one line, when there is no .venv."""

    def test_full_gate_without_venv_stops_before_cargo_with_one_line(self) -> None:
        gate = self.copy_script(GATE)
        self.fake_tool("cargo")

        result = self.run_script(gate)

        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(result.stderr.strip().splitlines(), [result.stderr.strip()])
        self.assertIn("./scripts/setup-venv.sh", result.stderr)
        self.assertEqual(self.calls(), [], "no cargo step may run before the venv check")

    def test_gate_never_falls_back_to_system_python(self) -> None:
        text = GATE.read_text(encoding="utf-8")
        py_body = re.search(r"^py\(\) \{\n(.*?)^\}", text, re.MULTILINE | re.DOTALL)
        assert py_body is not None
        self.assertNotIn("python3", py_body.group(1))


class SetupVenvTests(_Sandbox):
    """setup-venv.sh builds .venv the way CI's Python job installs its tools."""

    def _fake_python3(self) -> None:
        # `python3 -m venv .venv` creates .venv/bin/python as a recorder of its own.
        recorder = f'#!/usr/bin/env bash\necho "venv-python $*" >> "{self.log}"\n'
        self.fake_tool(
            "python3",
            'if [[ "$1 $2" == "-m venv" ]]; then dir="${@: -1}"; '
            'if [[ "$3" == "--clear" ]]; then rm -rf "$dir"; fi; mkdir -p "$dir/bin"; '
            f"printf '%s' '{recorder}' > \"$dir/bin/python\"; chmod +x \"$dir/bin/python\"; fi",
        )

    def test_setup_venv_installs_ci_groups(self) -> None:
        setup = self.copy_script(SETUP)
        self._fake_python3()

        result = self.run_script(setup)

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(
            self.calls(),
            [
                "python3 -m venv --clear .venv",
                "venv-python -m pip install --upgrade pip>=25.1",
                "venv-python -m pip install --group scripts/pyproject.toml:test --group scripts/pyproject.toml:dev",
            ],
        )

    def test_setup_venv_reuses_existing_venv(self) -> None:
        setup = self.copy_script(SETUP)
        self._fake_python3()
        self.run_script(setup)
        self.log.unlink()

        result = self.run_script(setup)

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn("python3 -m venv --clear .venv", self.calls())
        self.assertEqual(len(self.calls()), 2)

    def test_setup_venv_rebuilds_venv_whose_interpreter_is_gone(self) -> None:
        # A Homebrew upgrade that removes the old Python leaves .venv/bin/python dangling.
        # `python -m venv` without --clear keeps the dangling link, so pip then cannot start.
        setup = self.copy_script(SETUP)
        self._fake_python3()
        (self.repo / ".venv/bin").mkdir(parents=True)
        (self.repo / ".venv/bin/python").symlink_to("/nonexistent/python3.13")

        result = self.run_script(setup)

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.calls()[0], "python3 -m venv --clear .venv")
        self.assertEqual(len(self.calls()), 3)


class DeclarationTests(unittest.TestCase):
    """The worktree install builds the venv, and it installs what CI installs."""

    def test_install_shell_builds_the_venv(self) -> None:
        conf = (ROOT / ".cerebro/project.conf").read_text(encoding="utf-8")
        match = re.search(r"^install_shell\s+([^#\n]*)", conf, re.MULTILINE)
        assert match is not None
        self.assertTrue(match.group(1).strip().endswith("./scripts/setup-venv.sh"), match.group(1))

    def test_setup_venv_installs_the_groups_ci_installs(self) -> None:
        ci = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        groups = set(re.findall(r"--group (scripts/pyproject\.toml:\w+)", ci))
        self.assertTrue(groups)
        setup = SETUP.read_text(encoding="utf-8")
        for group in sorted(groups):
            with self.subTest(group=group):
                self.assertIn(f"--group {group}", setup)


if __name__ == "__main__":
    unittest.main()
