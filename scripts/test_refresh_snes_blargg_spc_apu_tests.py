"""Tests for scripts.refresh_snes_blargg_spc_apu_tests."""

from __future__ import annotations

import io
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from tempfile import TemporaryDirectory

from scripts.refresh_snes_blargg_spc_apu_tests import (
    DEFAULT_DEST_ROOT,
    REPO_ROOT,
    plan_refresh,
    run_refresh,
)


class TestRefreshSnesBlarggSpcApuTests(unittest.TestCase):
    """CLI/argument behavior for the optional-local SNES SPC/APU ROM corpus refresher."""

    def test_default_dest_root_is_repository_relative_optional_path(self) -> None:
        expected = (
            REPO_ROOT
            / "roms/snes/automated_tests/rom_pass_fail/blargg_spc_apu/full/v1"
        )

        self.assertEqual(DEFAULT_DEST_ROOT, expected)

    def test_plan_refresh_reports_no_network_action_when_dry_run(self) -> None:
        with TemporaryDirectory() as tmp:
            dest = Path(tmp) / "full" / "v1"

            plan = plan_refresh(dest_root=dest, dry_run=True)

            self.assertTrue(plan.dry_run)
            self.assertEqual(plan.dest_root, dest)
            self.assertFalse(plan.fetched_files)

    def test_run_refresh_dry_run_only_reports_plan_without_writing_files(
        self,
    ) -> None:
        with TemporaryDirectory() as tmp:
            dest = Path(tmp) / "full" / "v1"

            buf = io.StringIO()
            with redirect_stdout(buf):
                exit_code = run_refresh(["--dest-root", str(dest), "--dry-run"])

            self.assertEqual(exit_code, 0)
            self.assertFalse(dest.exists())
            self.assertIn("dry-run", buf.getvalue().lower())
