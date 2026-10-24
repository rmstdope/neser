"""Tests for the web build and serve commands: ``find_wasm_bindgen.sh`` and the pnpm package scripts.

They decide where the web integration suite gets its tools and its port, so a fleet
session can run the suite in its own worktree without a throwaway configuration (nr-bsr).
"""

import json
import os
import re
import socket
import stat
import subprocess
import tempfile
import time
import tomllib
import unittest
import urllib.request
from pathlib import Path

SCRIPTS_ROOT = Path(__file__).resolve().parent
REPO_ROOT = SCRIPTS_ROOT.parent
FIND_WASM_BINDGEN = SCRIPTS_ROOT / "find_wasm_bindgen.sh"


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


def _package_script(name: str) -> str:
    """Return the command ``pnpm run <name>`` runs, as package.json declares it."""
    scripts: dict[str, str] = json.loads((REPO_ROOT / "package.json").read_text(encoding="utf-8"))["scripts"]
    return scripts[name]


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


class ServeScriptTest(unittest.TestCase):
    """Given a built dist/, when ``pnpm run serve`` serves it, then it listens on NESER_WEB_PORT."""

    def _start(self, root: Path, port: int) -> subprocess.Popen[bytes]:
        env = dict(os.environ, NESER_WEB_PORT=str(port))
        return subprocess.Popen(
            ["sh", "-c", _package_script("serve")],
            cwd=root,
            env=env,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            start_new_session=True,
        )

    def test_serves_dist_and_the_rom_manifest_on_port_from_env(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "dist").mkdir()
            (root / "dist" / "index.html").write_text("neser-test-page")
            (root / "web" / "roms").mkdir(parents=True)
            (root / "web" / "roms" / "roms.json").write_text('{"roms":[]}')
            port = _free_port()
            server = self._start(root, port)
            try:
                page = _fetch_when_up(f"http://127.0.0.1:{port}/index.html")
                manifest = _fetch_when_up(f"http://127.0.0.1:{port}/roms/roms.json")
            finally:
                os.killpg(server.pid, 15)
                server.wait(timeout=10)

            self.assertEqual("neser-test-page", page)
            self.assertEqual('{"roms":[]}', manifest)

    def test_refuses_without_a_build(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = subprocess.run(
                ["sh", "-c", _package_script("serve")], cwd=tmp, capture_output=True, text=True, check=False
            )
            self.assertNotEqual(0, result.returncode)
            self.assertIn("pnpm run build", result.stderr)


class BuildScriptsTest(unittest.TestCase):
    """Given the package scripts, when they build, then web/pkg carries the bindings' own types (nr-n48).

    pnpm runs a script with ``sh -c``; here ``cargo``, ``wasm-bindgen``, ``wasm-opt``, ``vite``
    and ``pnpm`` are fakes on ``PATH`` that only log their arguments, so the test sees every
    command a script runs without compiling anything.
    """

    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = Path(self._tmp.name)
        self.log = self.tmp / "commands.log"
        bin_dir = self.tmp / "bin"
        bin_dir.mkdir()
        for tool in ("cargo", "wasm-bindgen", "wasm-opt", "vite", "pnpm"):
            path = bin_dir / tool
            body = f'echo "{tool} $*" >> "$COMMAND_LOG"\n'
            if tool == "cargo":
                body += 'echo "RUSTUP_TOOLCHAIN=${RUSTUP_TOOLCHAIN-unset}" >> "$COMMAND_LOG"\n'
            path.write_text("#!/bin/sh\n" + body)
            path.chmod(path.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
        self.path = f"{bin_dir}:/usr/bin:/bin"

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def _run(self, script: str) -> list[str]:
        env = {
            "PATH": self.path,
            "HOME": str(self.tmp),
            "COMMAND_LOG": str(self.log),
            "RUSTUP_TOOLCHAIN": "stable",
        }
        result = subprocess.run(
            ["sh", "-c", _package_script(script)],
            cwd=REPO_ROOT,
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(0, result.returncode, result.stderr)
        return self.log.read_text().splitlines()

    def test_build_wasm_builds_binds_and_optimises_web_pkg(self) -> None:
        commands = self._run("build:wasm")
        self.assertTrue(commands[0].startswith("cargo build --profile wasm-release"), commands)
        self.assertIn("--target wasm32-unknown-unknown --no-default-features --features wasm", commands[0])
        self.assertTrue(commands[2].startswith("wasm-bindgen "), commands)
        self.assertEqual("wasm-opt -O3 web/pkg/neser_bg.wasm -o web/pkg/neser_bg.wasm", commands[3])
        self.assertEqual(4, len(commands), commands)

    def test_build_wasm_runs_cargo_on_the_pinned_toolchain(self) -> None:
        """rustup's cargo exports RUSTUP_TOOLCHAIN, which beats rust-toolchain.toml; it must not leak in."""
        self.assertEqual("RUSTUP_TOOLCHAIN=unset", self._run("build:wasm")[1])

    def test_bindings_are_generated_with_typescript(self) -> None:
        bindgen = [line for line in self._run("build:wasm") if line.startswith("wasm-bindgen ")]
        self.assertEqual(1, len(bindgen), bindgen)
        self.assertIn("target/wasm32-unknown-unknown/wasm-release/neser.wasm", bindgen[0])
        self.assertIn("--out-dir web/pkg", bindgen[0])
        self.assertNotIn("--no-typescript", bindgen[0])

    def test_build_builds_the_wasm_then_bundles_with_vite(self) -> None:
        self.assertEqual(["pnpm run build:wasm", "vite build"], self._run("build"))

    def test_start_builds_then_serves(self) -> None:
        self.assertEqual(["pnpm run build", "pnpm run serve"], self._run("start"))

    def test_typecheck_checks_web_against_tsconfig(self) -> None:
        self.assertEqual("tsc --noEmit -p tsconfig.json", _package_script("typecheck"))

    def test_build_wasm_dev_builds_the_dev_profile_without_wasm_opt(self) -> None:
        """A Rust save during `dev` rebuilds in seconds: no LTO profile and no wasm-opt (nr-fli1.2)."""
        commands = self._run("build:wasm:dev")
        self.assertTrue(commands[0].startswith("cargo build --profile wasm-dev"), commands)
        self.assertIn("--target wasm32-unknown-unknown --no-default-features --features wasm", commands[0])
        self.assertEqual("RUSTUP_TOOLCHAIN=unset", commands[1])
        self.assertTrue(commands[2].startswith("wasm-bindgen "), commands)
        self.assertIn("target/wasm32-unknown-unknown/wasm-dev/neser.wasm", commands[2])
        self.assertIn("--out-dir web/pkg", commands[2])
        self.assertEqual(3, len(commands), commands)

    def test_dev_builds_the_dev_wasm_then_starts_vite(self) -> None:
        self.assertEqual(["pnpm run build:wasm:dev", "vite"], [c.rstrip() for c in self._run("dev")])


class WasmDevProfileTest(unittest.TestCase):
    """The profile `build:wasm:dev` compiles is fast to rebuild and still optimised (nr-fli1.2)."""

    def test_wasm_dev_profile_is_incremental_release_without_lto(self) -> None:
        profile = tomllib.loads((REPO_ROOT / "Cargo.toml").read_text(encoding="utf-8"))["profile"]["wasm-dev"]
        self.assertEqual("release", profile["inherits"])
        self.assertIs(False, profile["lto"])
        self.assertIs(True, profile["incremental"])
        self.assertGreater(profile["codegen-units"], 1)


class PnpmOnlyTest(unittest.TestCase):
    """The web frontend is built, run and tested through pnpm, and nothing else (nr-fli1.1)."""

    def test_the_bash_build_and_serve_scripts_are_gone(self) -> None:
        self.assertFalse((SCRIPTS_ROOT / "build_web.sh").exists())
        self.assertFalse((SCRIPTS_ROOT / "run_web.sh").exists())

    def test_pnpm_owns_the_lockfile(self) -> None:
        self.assertTrue((REPO_ROOT / "pnpm-lock.yaml").exists())
        self.assertFalse((REPO_ROOT / "package-lock.json").exists())
        package = json.loads((REPO_ROOT / "package.json").read_text(encoding="utf-8"))
        self.assertRegex(package["packageManager"], r"^pnpm@\d+\.\d+\.\d+$")

    def test_no_runner_of_the_web_frontend_calls_npm_or_the_old_scripts(self) -> None:
        for relative in (
            ".github/workflows/ci.yml",
            "scripts/gate-full.sh",
            "scripts/deploy.py",
            "scripts/ensure_playwright_deps.sh",
            "playwright.config.ts",
            ".cerebro/project.conf",
        ):
            text = (REPO_ROOT / relative).read_text(encoding="utf-8")
            with self.subTest(file=relative):
                found = re.findall(r"\bnpm (?:ci|test|run|install)\b|\bnpx \S+|build_web|run_web|package-lock", text)
                self.assertEqual([], found)


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

    def test_job_builds_the_web_app_once_with_pnpm_build(self) -> None:
        job = "".join(self.steps)
        self.assertEqual(1, job.count("run: pnpm run build\n"))
        self.assertNotIn("cargo build", job)
        self.assertNotIn("wasm-bindgen target/", job)

    def test_test_step_serves_the_prebuilt_artifacts(self) -> None:
        test_step = self.steps[self._step("Run web integration tests")]
        self.assertRegex(test_step, r'\n {10}NESER_WEB_PREBUILT: "1"\n')

    def test_build_runs_before_the_tests(self) -> None:
        build = next(i for i, step in enumerate(self.steps) if "run: pnpm run build\n" in step)
        self.assertLess(build, self._step("Run web integration tests"))

    def test_typechecks_against_the_bindings_the_build_generated(self) -> None:
        """tsc needs web/pkg/neser.d.ts, which only the build step writes (nr-n48)."""
        typecheck = self._step("Type-check web frontend")
        self.assertIn("run: pnpm run typecheck\n", self.steps[typecheck])
        self.assertLess(self._step("Build web app"), typecheck)

    def test_a_tsconfig_change_runs_the_job(self) -> None:
        ci = (REPO_ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")
        web_integration_filter = re.search(r"\n {12}web_integration:\n((?: {14}- .*\n)+)", ci)
        assert web_integration_filter is not None, "ci.yml has no web_integration path filter"
        self.assertIn("- 'tsconfig.json'", web_integration_filter.group(1))

    def test_a_vite_config_change_runs_the_ci_typecheck(self) -> None:
        """tsconfig.json includes vite.config.ts, so the job that runs tsc must see it change."""
        ci = (REPO_ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")
        web_integration_filter = re.search(r"\n {12}web_integration:\n((?: {14}- .*\n)+)", ci)
        assert web_integration_filter is not None, "ci.yml has no web_integration path filter"
        self.assertIn("- 'vite.config.ts'", web_integration_filter.group(1))


class GateTypecheckTest(unittest.TestCase):
    """Given the full gate, when it reaches the web legs, then it type-checks the web TypeScript (nr-n48)."""

    def setUp(self) -> None:
        self.gate = (SCRIPTS_ROOT / "gate-full.sh").read_text(encoding="utf-8")

    def _line(self, command: str) -> int:
        lines = self.gate.splitlines()
        self.assertIn(command, lines)
        return lines.index(command)

    def test_builds_the_bindings_then_typechecks_before_the_unit_tests(self) -> None:
        build = self._line("step pnpm run build:wasm")
        typecheck = self._line("step pnpm run typecheck")
        self.assertLess(build, typecheck)
        self.assertLess(typecheck, self._line("step pnpm test"))


if __name__ == "__main__":
    unittest.main()
