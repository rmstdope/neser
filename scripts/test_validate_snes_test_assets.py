"""Tests for scripts.validate_snes_test_assets."""

from __future__ import annotations

import copy
import unittest

from scripts.validate_snes_test_assets import load_manifest, validate_manifest


class TestValidateSnesTestAssets(unittest.TestCase):
    """Schema and policy validation checks for the SNES asset manifest."""

    def test_current_manifest_is_valid(self) -> None:
        """The committed manifest should pass validation unchanged."""

        manifest = load_manifest()
        self.assertEqual(validate_manifest(manifest), [])

    def test_missing_license_fails(self) -> None:
        """Asset license metadata is mandatory."""

        manifest = load_manifest()
        modified = copy.deepcopy(manifest)
        modified["assets"][0]["license"] = ""

        errors = validate_manifest(modified)

        self.assertTrue(any("license" in error for error in errors))

    def test_duplicate_asset_id_fails(self) -> None:
        """Asset identifiers must remain globally unique."""

        manifest = load_manifest()
        modified = copy.deepcopy(manifest)
        modified["assets"][1]["id"] = modified["assets"][0]["id"]

        errors = validate_manifest(modified)

        self.assertTrue(any("duplicate asset id" in error for error in errors))

    def test_optional_variant_requires_refresh_command(self) -> None:
        """Optional local assets must define a refresh or fetch command."""

        manifest = load_manifest()
        modified = copy.deepcopy(manifest)
        modified["assets"][0]["variants"][1]["refresh_command"] = ""

        errors = validate_manifest(modified)

        self.assertTrue(any("optional_local variants require refresh_command" in error for error in errors))

    def test_tree_integrity_requires_hex_sha256(self) -> None:
        """Committed tree hashes must use full 64-hex SHA-256 values."""

        manifest = load_manifest()
        modified = copy.deepcopy(manifest)
        modified["assets"][0]["variants"][0]["integrity"]["sha256"] = "1234"

        errors = validate_manifest(modified)

        self.assertTrue(any("64-char hex" in error for error in errors))

    def test_not_vendored_requires_not_applicable_checksum(self) -> None:
        """Non-vendored variants keep an explicit not_applicable checksum marker."""

        manifest = load_manifest()
        modified = copy.deepcopy(manifest)
        modified["assets"][1]["variants"][1]["integrity"]["sha256"] = "abcdef"

        errors = validate_manifest(modified)

        self.assertTrue(any("not_vendored integrity" in error for error in errors))


if __name__ == "__main__":
    unittest.main()
