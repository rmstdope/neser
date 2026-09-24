"""Regression tests for the pinned Rust toolchain (rust-toolchain.toml).

CI once went red on unchanged code because it installed whatever stable Rust
was current and a new release shipped new clippy lints. The toolchain is now
pinned in one file that rustup honours locally and in CI; these tests keep
it pinned and keep the workflows from overriding it.
"""

import re
import tomllib
import unittest
from pathlib import Path

ROOT = Path(__file__).parents[1]
TOOLCHAIN_FILE = ROOT / "rust-toolchain.toml"
WORKFLOWS_DIR = ROOT / ".github/workflows"


def _toolchain() -> dict[str, object]:
    with TOOLCHAIN_FILE.open("rb") as handle:
        table = tomllib.load(handle)["toolchain"]
    assert isinstance(table, dict)
    return table


class RustToolchainPinTests(unittest.TestCase):
    """The toolchain file pins one exact version that CI and local builds share."""

    def test_toolchain_file_pins_exact_stable_version(self) -> None:
        """The channel is an exact x.y.z release, with rustfmt and clippy."""

        toolchain = _toolchain()
        self.assertRegex(str(toolchain["channel"]), r"^\d+\.\d+\.\d+$")
        components = toolchain["components"]
        assert isinstance(components, list)
        self.assertIn("rustfmt", components)
        self.assertIn("clippy", components)

    def test_toolchain_file_includes_wasm_target(self) -> None:
        """The wasm target is installed with the toolchain for clippy and wasm-pack."""

        targets = _toolchain()["targets"]
        assert isinstance(targets, list)
        self.assertIn("wasm32-unknown-unknown", targets)

    def test_workflows_do_not_override_pinned_toolchain(self) -> None:
        """No workflow installs or selects a toolchain other than the pinned one."""

        overrides = re.compile(
            r"dtolnay/rust-toolchain|actions-rs/toolchain|RUSTUP_TOOLCHAIN"
            r"|rustup\s+(default|override)|--toolchain|cargo \+"
            r"|rustup[^\n]*\b(stable|beta|nightly)\b|toolchain:"
        )
        workflows = sorted([*WORKFLOWS_DIR.glob("*.yml"), *WORKFLOWS_DIR.glob("*.yaml")])
        self.assertTrue(workflows)
        for workflow in workflows:
            with self.subTest(workflow=workflow.name):
                text = workflow.read_text(encoding="utf-8")
                self.assertIsNone(overrides.search(text))

    def test_ci_runs_every_rust_suite_when_the_pin_changes(self) -> None:
        """A toolchain bump changes codegen everywhere, so it triggers the full Rust suites."""

        ci = (WORKFLOWS_DIR / "ci.yml").read_text(encoding="utf-8")
        for group in ("rust", "root_rust", "web_integration"):
            with self.subTest(group=group):
                match = re.search(rf"^ {{12}}{group}:\n((?: {{14}}.*\n)+)", ci, re.MULTILINE)
                assert match is not None, group
                self.assertIn("- 'rust-toolchain.toml'", match.group(1))

    def test_gate_ignores_inherited_toolchain_override(self) -> None:
        """Each repo script that runs cargo unsets RUSTUP_TOOLCHAIN, which beats the pin."""

        for script, first_cargo in (
            ("scripts/gate-full.sh", "\nstep cargo"),
            ("scripts/test-dir.sh", "CMD=(cargo test"),
            (".githooks/pre-commit", "    cargo fmt\n"),
        ):
            with self.subTest(script=script):
                text = (ROOT / script).read_text(encoding="utf-8")
                unset = text.index("unset RUSTUP_TOOLCHAIN")
                self.assertLess(unset, text.index(first_cargo))

    def test_readme_documents_toolchain_bump(self) -> None:
        """README.md explains how to bump the pinned toolchain."""

        readme = (ROOT / "README.md").read_text(encoding="utf-8")
        self.assertIn("rust-toolchain.toml", readme)
        self.assertIn("### Bumping the Rust toolchain", readme)


if __name__ == "__main__":
    unittest.main()
