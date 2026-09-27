"""Tests for the web build and serve scripts: ``find_wasm_bindgen.sh`` and ``run_web.sh``.

Both scripts decide where the web integration suite gets its tools and its port, so a fleet
session can run the suite in its own worktree without a throwaway configuration (nr-bsr).
"""

import os
import re
import socket
import stat
import subprocess
import tempfile
import time
import unittest
import urllib.request
from pathlib import Path

SCRIPTS_ROOT = Path(__file__).resolve().parent
REPO_ROOT = SCRIPTS_ROOT.parent
FIND_WASM_BINDGEN = SCRIPTS_ROOT / "find_wasm_bindgen.sh"
RUN_WEB = SCRIPTS_ROOT / "run_web.sh"


def _locked_wasm_bindgen_version() -> str:
    """Return the wasm-bindgen version Cargo.lock pins, the one the CLI must match."""
    lock = (REPO_ROOT / "Cargo.lock").read_text()
    match = re.search(r'name = "wasm-bindgen"\nversion = "([^"]+)"', lock)
    assert match, "Cargo.lock names no wasm-bindgen package"
    return match.group(1)


def _fake_wasm_bindgen(directory: Path, version: str) -> Path:
    """Create an executable that answers ``--version`` like the real wasm-bindgen CLI."""
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / "wasm-bindgen"
    path.write_text(f'#!/bin/sh\necho "wasm-bindgen {version}"\n')
    path.chmod(path.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
    return path


class FindWasmBindgenTest(unittest.TestCase):
    """Given where wasm-bindgen is installed, when the build looks for it, then it finds the right one."""

    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = Path(self._tmp.name)
        self.version = _locked_wasm_bindgen_version()
        self.cache = self.tmp / "cache"
        self.cache.mkdir()
        # A PATH with the basic tools the script needs and no wasm-bindgen.
        self.bare_path = "/usr/bin:/bin"

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def _run(self, path: str) -> subprocess.CompletedProcess[str]:
        env = {"PATH": path, "HOME": str(self.tmp), "WASM_PACK_CACHE": str(self.cache)}
        return subprocess.run(["sh", str(FIND_WASM_BINDGEN)], env=env, capture_output=True, text=True, check=False)

    def test_prefers_wasm_bindgen_on_path(self) -> None:
        on_path = _fake_wasm_bindgen(self.tmp / "bin", self.version)
        _fake_wasm_bindgen(self.cache / f"wasm-bindgen-cargo-install-{self.version}", self.version)

        result = self._run(f"{on_path.parent}:{self.bare_path}")

        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual(str(on_path), result.stdout.strip())

    def test_falls_back_to_wasm_pack_cache_with_locked_version(self) -> None:
        _fake_wasm_bindgen(self.cache / "wasm-bindgen-cargo-install-0.0.1", "0.0.1")
        cached = _fake_wasm_bindgen(self.cache / f"wasm-bindgen-cargo-install-{self.version}", self.version)

        result = self._run(self.bare_path)

        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual(str(cached), result.stdout.strip())

    def test_refuses_cached_wasm_bindgen_of_another_version(self) -> None:
        _fake_wasm_bindgen(self.cache / "wasm-bindgen-cargo-install-0.0.1", "0.0.1")

        result = self._run(self.bare_path)

        self.assertNotEqual(0, result.returncode)
        self.assertEqual("", result.stdout)
        self.assertIn(self.version, result.stderr)


def _free_port() -> int:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port: int = sock.getsockname()[1]
        return port


class RunWebPortTest(unittest.TestCase):
    """Given NESER_WEB_PORT, when run_web.sh serves dist/, then it listens on that port."""

    def test_serves_on_port_from_env(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "dist").mkdir()
            (root / "dist" / "index.html").write_text("neser-test-page")
            (root / "web" / "roms").mkdir(parents=True)
            port = _free_port()
            env = dict(os.environ, NESER_WEB_PORT=str(port))
            server = subprocess.Popen(
                ["sh", str(RUN_WEB)],
                cwd=root,
                env=env,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                start_new_session=True,
            )
            try:
                body = self._fetch_when_up(f"http://127.0.0.1:{port}/index.html")
            finally:
                os.killpg(server.pid, 15)
                server.wait(timeout=10)

            self.assertEqual("neser-test-page", body)

    @staticmethod
    def _fetch_when_up(url: str) -> str:
        deadline = time.monotonic() + 10
        while True:
            try:
                with urllib.request.urlopen(url, timeout=1) as response:
                    text: str = response.read().decode()
                    return text
            except OSError:
                if time.monotonic() > deadline:
                    raise
                time.sleep(0.1)


def _web_integration_steps() -> list[str]:
    """Return the text of each step of the ``web-integration`` CI job, in order."""
    ci = (REPO_ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")
    job = re.search(r"^  web-integration:\n((?:(?:    .*)?\n)+)", ci, re.MULTILINE)
    assert job is not None, "ci.yml has no web-integration job"
    steps_block = job.group(1).split("    steps:\n", 1)[1]
    return [step for step in re.split(r"^      - ", steps_block, flags=re.MULTILINE) if step.strip()]


class WebIntegrationJobTest(unittest.TestCase):
    """Given the web-integration CI job, when it runs, then it compiles the wasm exactly once (nr-1pl)."""

    def setUp(self) -> None:
        self.steps = _web_integration_steps()

    def _step(self, name: str) -> int:
        for index, step in enumerate(self.steps):
            if step.startswith(f"name: {name}\n"):
                return index
        self.fail(f"web-integration has no step named {name!r}")

    def test_job_builds_the_web_app_once_with_build_web_sh(self) -> None:
        job = "".join(self.steps)
        self.assertEqual(1, job.count("bash scripts/build_web.sh"))
        self.assertNotIn("cargo build", job)
        self.assertNotIn("wasm-bindgen target/", job)

    def test_test_step_serves_the_prebuilt_artifacts(self) -> None:
        test_step = self.steps[self._step("Run web integration tests")]
        self.assertRegex(test_step, r'\n {10}SKIP_WASM_BUILD_IF_ARTIFACTS_EXIST: "1"\n')

    def test_build_runs_before_the_tests(self) -> None:
        build = next(i for i, step in enumerate(self.steps) if "bash scripts/build_web.sh" in step)
        self.assertLess(build, self._step("Run web integration tests"))


if __name__ == "__main__":
    unittest.main()
