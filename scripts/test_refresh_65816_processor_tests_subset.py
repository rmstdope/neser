"""Tests for scripts.refresh_65816_processor_tests_subset."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from scripts.refresh_65816_processor_tests_subset import (
    DEFAULT_REPORT_JSON,
    FAMILY_ORDER,
    OPCODE_FAMILY,
    REPO_ROOT,
    _materialize_payload,
    build_report,
    discover_vector_files,
    select_subset_files,
    write_subset,
)


def _touch(path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("[]\n", encoding="utf-8")


def _write_vectors(path: Path, count: int) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    vectors = [{"name": f"case-{i}"} for i in range(count)]
    path.write_text(json.dumps(vectors), encoding="utf-8")


class TestRefresh65816ProcessorTestsSubset(unittest.TestCase):
    """Selection and reporting behavior for committed 65816 subset generation."""

    def test_selects_both_modes_for_selected_opcode(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            full = Path(tmp) / "full"
            _touch(full / "00.e.json")
            _touch(full / "00.n.json")
            _touch(full / "20.e.json")
            _touch(full / "20.n.json")
            _touch(full / "4c.e.json")
            _touch(full / "4c.n.json")
            _touch(full / "69.e.json")
            _touch(full / "69.n.json")
            _touch(full / "0a.e.json")
            _touch(full / "0a.n.json")
            _touch(full / "18.e.json")
            _touch(full / "18.n.json")
            _touch(full / "44.e.json")
            _touch(full / "44.n.json")

            files = discover_vector_files(full)
            selected = select_subset_files(files, opcodes_per_family=1)
            names = [item.filename for item in selected]

            self.assertIn("00.e.json", names)
            self.assertIn("00.n.json", names)
            self.assertIn("69.e.json", names)
            self.assertIn("69.n.json", names)
            self.assertIn("44.e.json", names)
            self.assertIn("44.n.json", names)

    def test_selection_is_deterministic(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            full = Path(tmp) / "full"
            for name in (
                "69.n.json",
                "69.e.json",
                "00.n.json",
                "00.e.json",
                "4c.n.json",
                "4c.e.json",
                "20.n.json",
                "20.e.json",
                "0a.n.json",
                "0a.e.json",
                "18.n.json",
                "18.e.json",
                "44.n.json",
                "44.e.json",
            ):
                _touch(full / name)

            files = discover_vector_files(full)

            first = [item.filename for item in select_subset_files(files, opcodes_per_family=1)]
            second = [item.filename for item in select_subset_files(files, opcodes_per_family=1)]

            self.assertEqual(first, second)

    def test_write_subset_replaces_previous_json_files(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            full = Path(tmp) / "full"
            subset = Path(tmp) / "subset"

            _touch(full / "00.e.json")
            _touch(full / "00.n.json")
            _touch(full / "4c.e.json")
            _touch(full / "4c.n.json")
            _touch(full / "20.e.json")
            _touch(full / "20.n.json")
            _touch(full / "69.e.json")
            _touch(full / "69.n.json")
            _touch(full / "0a.e.json")
            _touch(full / "0a.n.json")
            _touch(full / "18.e.json")
            _touch(full / "18.n.json")
            _touch(full / "44.e.json")
            _touch(full / "44.n.json")

            subset.mkdir(parents=True, exist_ok=True)
            _touch(subset / "stale.json")

            selected = select_subset_files(discover_vector_files(full), opcodes_per_family=1)
            write_subset(selected, subset)

            self.assertFalse((subset / "stale.json").exists())
            copied = sorted(path.name for path in subset.glob("*.json"))
            self.assertEqual(copied, sorted(item.filename for item in selected))

    def test_build_report_contains_family_coverage(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            full = Path(tmp) / "full"
            _touch(full / "00.e.json")
            _touch(full / "00.n.json")
            _touch(full / "69.e.json")
            _touch(full / "69.n.json")

            selected = select_subset_files(discover_vector_files(full), opcodes_per_family=1)
            report = build_report(selected)

            self.assertIn("selected_files", report)
            self.assertIn("family_coverage", report)
            self.assertIn("integrity", report)
            self.assertIn("system_control", report["family_coverage"])
            self.assertIn("alu", report["family_coverage"])
            self.assertEqual(report["integrity"]["kind"], "tree_sha256")
            self.assertEqual(report["integrity"]["file_count"], len(selected))
            self.assertGreater(report["integrity"]["total_size_bytes"], 0)

            json.dumps(report)

    def test_cycle_exact_opcodes_each_get_their_own_family(self) -> None:
        """Each direct-page mode and RMW form must survive selection independently.

        The per-cycle bus assertions in ``processor_tests_65816.rs`` can only run against
        opcodes that are actually committed. Giving each mode its own family is what keeps
        one representative of each in the subset -- if they shared a family, selection would
        keep only one of them and silently drop coverage for the rest.
        """
        cycle_exact = {
            0xA5: "dp_direct",
            0xB5: "dp_indexed_x",
            0xB6: "dp_indexed_y",
            0xA1: "dp_indexed_indirect",
            0xB2: "dp_indirect",
            0xB1: "dp_indirect_indexed",
            0xA7: "dp_indirect_long",
            0xB7: "dp_indirect_long_indexed",
            0xD4: "dp_push_indirect",
            0x06: "rmw_direct",
            0x16: "rmw_direct_indexed",
            0x0E: "rmw_absolute",
            0x1E: "rmw_absolute_indexed",
            0x04: "rmw_test_bits",
        }

        with tempfile.TemporaryDirectory() as tmp:
            full = Path(tmp) / "full"
            for opcode in cycle_exact:
                for mode in ("e", "n"):
                    _touch(full / f"{opcode:02x}.{mode}.json")

            selected = select_subset_files(discover_vector_files(full), opcodes_per_family=1)
            selected_opcodes = {item.opcode for item in selected}

            for opcode, family in cycle_exact.items():
                self.assertEqual(OPCODE_FAMILY[opcode], family)
                self.assertIn(family, FAMILY_ORDER)
                self.assertIn(opcode, selected_opcodes)

            # Both the emulation and native vector files are kept for each.
            self.assertEqual(len(selected), 2 * len(cycle_exact))

    def test_write_subset_can_truncate_vectors_per_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            full = Path(tmp) / "full"
            subset = Path(tmp) / "subset"

            _write_vectors(full / "00.e.json", 10)
            _write_vectors(full / "00.n.json", 10)
            _write_vectors(full / "4c.e.json", 10)
            _write_vectors(full / "4c.n.json", 10)
            _write_vectors(full / "20.e.json", 10)
            _write_vectors(full / "20.n.json", 10)
            _write_vectors(full / "69.e.json", 10)
            _write_vectors(full / "69.n.json", 10)
            _write_vectors(full / "0a.e.json", 10)
            _write_vectors(full / "0a.n.json", 10)
            _write_vectors(full / "18.e.json", 10)
            _write_vectors(full / "18.n.json", 10)
            _write_vectors(full / "44.e.json", 10)
            _write_vectors(full / "44.n.json", 10)

            selected = select_subset_files(discover_vector_files(full), opcodes_per_family=1)
            write_subset(selected, subset, max_vectors_per_file=3)

            for path in subset.glob("*.json"):
                payload = json.loads(path.read_text(encoding="utf-8"))
                self.assertEqual(len(payload), 3)

    def test_build_report_integrity_uses_truncated_payloads(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            full = Path(tmp) / "full"
            _write_vectors(full / "00.e.json", 10)
            _write_vectors(full / "00.n.json", 10)
            _write_vectors(full / "69.e.json", 10)
            _write_vectors(full / "69.n.json", 10)

            selected = select_subset_files(discover_vector_files(full), opcodes_per_family=1)
            full_report = build_report(selected)
            truncated_report = build_report(selected, max_vectors_per_file=2)

            self.assertNotEqual(
                full_report["integrity"]["sha256"],
                truncated_report["integrity"]["sha256"],
            )
            self.assertLess(
                truncated_report["integrity"]["total_size_bytes"],
                full_report["integrity"]["total_size_bytes"],
            )

    def test_max_vectors_zero_disables_truncation(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            full = Path(tmp) / "full"
            path = full / "00.e.json"
            _write_vectors(path, 5)

            files = discover_vector_files(full)
            self.assertEqual(len(files), 1)

            payload = _materialize_payload(files[0], max_vectors_per_file=0)
            vectors = json.loads(payload.decode("utf-8"))

            self.assertEqual(len(vectors), 5)

    def test_negative_max_vectors_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            full = Path(tmp) / "full"
            path = full / "00.e.json"
            _write_vectors(path, 5)

            files = discover_vector_files(full)
            self.assertEqual(len(files), 1)

            with self.assertRaises(ValueError):
                _materialize_payload(files[0], max_vectors_per_file=-1)

    def test_default_report_path_targets_committed_report_file(self) -> None:
        expected = REPO_ROOT / "roms/snes/automated_tests/processor_tests/65816/subset_coverage_report.json"
        self.assertEqual(DEFAULT_REPORT_JSON, expected)

    def test_dry_run_does_not_write_report_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            full = Path(tmp) / "full"
            subset = Path(tmp) / "subset"
            report = Path(tmp) / "subset_coverage_report.json"

            for name in (
                "ea.e.json",
                "ea.n.json",
                "d0.e.json",
                "d0.n.json",
                "48.e.json",
                "48.n.json",
                "a9.e.json",
                "a9.n.json",
                "3b.e.json",
                "3b.n.json",
                "4a.e.json",
                "4a.n.json",
                "58.e.json",
                "58.n.json",
                "44.e.json",
                "44.n.json",
            ):
                _write_vectors(full / name, 2)

            result = subprocess.run(
                [
                    sys.executable,
                    "-m",
                    "scripts.refresh_65816_processor_tests_subset",
                    "--full-root",
                    str(full),
                    "--subset-root",
                    str(subset),
                    "--report-json",
                    str(report),
                    "--dry-run",
                ],
                cwd=REPO_ROOT,
                capture_output=True,
                text=True,
                check=False,
            )

            self.assertEqual(result.returncode, 0, msg=result.stderr)
            self.assertFalse(report.exists())
            self.assertFalse(subset.exists())


if __name__ == "__main__":
    unittest.main()
