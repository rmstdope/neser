"""Tests for scripts.compute_snes_rom_asset_integrity."""

from __future__ import annotations

import hashlib
import tempfile
import unittest
from pathlib import Path

from scripts.compute_snes_rom_asset_integrity import compute_integrity


class ComputeIntegrityTests(unittest.TestCase):
    def test_empty_directory_reports_zero_counts(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            integrity = compute_integrity(Path(tmp))

        self.assertEqual(integrity["kind"], "tree_sha256")
        self.assertEqual(integrity["file_count"], 0)
        self.assertEqual(integrity["total_size_bytes"], 0)
        self.assertRegex(integrity["sha256"], r"^[0-9a-f]{64}$")

    def test_known_files_match_reference_record_hash(self) -> None:
        content_a = b"\x00" * 16
        content_b = b"\x01" * 32
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "b.smc").write_bytes(content_b)
            (root / "a.sfc").write_bytes(content_a)

            integrity = compute_integrity(root)

        # Records are sorted by filename, joined by newlines, trailing newline.
        sha_a = hashlib.sha256(content_a).hexdigest()
        sha_b = hashlib.sha256(content_b).hexdigest()
        records = [f"{sha_a}  a.sfc", f"{sha_b}  b.smc"]
        expected = hashlib.sha256(
            ("\n".join(records) + "\n").encode("utf-8")
        ).hexdigest()

        self.assertEqual(integrity["file_count"], 2)
        self.assertEqual(integrity["total_size_bytes"], 48)
        self.assertEqual(integrity["sha256"], expected)

    def test_non_rom_files_are_ignored(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "rom.sfc").write_bytes(b"\x00" * 16)
            (root / "readme.txt").write_bytes(b"hello")

            integrity = compute_integrity(root)

        self.assertEqual(integrity["file_count"], 1)
        self.assertEqual(integrity["total_size_bytes"], 16)

    def test_is_deterministic(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "rom.sfc").write_bytes(b"\x42" * 64)

            self.assertEqual(compute_integrity(root), compute_integrity(root))


if __name__ == "__main__":
    unittest.main()
