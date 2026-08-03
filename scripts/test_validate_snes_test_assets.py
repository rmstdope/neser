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

    def test_variant_integrity_metadata_is_not_required(self) -> None:
        """Variants without an integrity object are accepted."""

        manifest = load_manifest()
        modified = copy.deepcopy(manifest)
        for asset in modified["assets"]:
            for variant in asset["variants"]:
                variant.pop("integrity", None)

        self.assertEqual(validate_manifest(modified), [])

    def test_variant_path_must_be_repository_relative(self) -> None:
        """Absolute variant paths are rejected."""

        manifest = load_manifest()
        modified = copy.deepcopy(manifest)
        modified["assets"][0]["variants"][0]["path"] = "/tmp/outside"

        errors = validate_manifest(modified)

        self.assertTrue(any("path must be repository-relative" in error for error in errors))

    def test_variant_path_must_not_contain_parent_segments(self) -> None:
        """Parent-directory traversal segments are rejected."""

        manifest = load_manifest()
        modified = copy.deepcopy(manifest)
        modified["assets"][0]["variants"][0]["path"] = "roms/snes/../outside"

        errors = validate_manifest(modified)

        self.assertTrue(any("must not contain '..'" in error for error in errors))

    def test_processor_tests_assets_with_same_source_url_require_shared_ref(
        self,
    ) -> None:
        """SNES processor_tests entries should pin one shared upstream ref per source URL."""

        manifest = load_manifest()
        modified = copy.deepcopy(manifest)
        modified["assets"][1]["source"]["ref"] = "1111111111111111111111111111111111111111"

        errors = validate_manifest(modified)

        self.assertTrue(any("processor_tests assets sharing source.url" in error for error in errors))

    def test_processor_tests_assets_with_different_source_urls_may_use_different_refs(
        self,
    ) -> None:
        """The shared-ref rule is scoped per source URL, not globally."""

        manifest = load_manifest()
        modified = copy.deepcopy(manifest)
        modified["assets"][1]["source"]["url"] = "https://example.invalid/alternate"
        modified["assets"][1]["source"]["ref"] = "1111111111111111111111111111111111111111"

        errors = validate_manifest(modified)

        self.assertFalse(any("processor_tests assets sharing source.url" in error for error in errors))

    def test_processor_tests_source_ref_must_be_40_char_lowercase_sha(self) -> None:
        """ProcessorTests assets should pin immutable commit SHAs, not branch names."""

        manifest = load_manifest()
        modified = copy.deepcopy(manifest)
        modified["assets"][0]["source"]["ref"] = "main"

        errors = validate_manifest(modified)

        self.assertTrue(
            any("processor_tests assets must use a 40-char lowercase commit SHA" in error for error in errors)
        )

    def test_non_processor_tests_assets_do_not_require_sha_ref_format(self) -> None:
        """Only processor_tests assets are constrained to SHA-like refs."""

        manifest = load_manifest()
        modified = copy.deepcopy(manifest)
        modified["assets"][0]["suite"] = "rom_pass_fail"
        modified["assets"][0]["source"]["ref"] = "main"

        errors = validate_manifest(modified)

        self.assertFalse(
            any("processor_tests assets must use a 40-char lowercase commit SHA" in error for error in errors)
        )

    def test_manifest_contains_at_least_one_rom_pass_fail_asset(self) -> None:
        """SNES manifest should track at least one ROM pass/fail suite entry."""

        manifest = load_manifest()

        self.assertTrue(
            any(asset.get("suite") == "rom_pass_fail" for asset in manifest.get("assets", [])),
            "expected at least one rom_pass_fail asset in SNES manifest",
        )

    def test_manifest_contains_dsp_audio_golden_asset(self) -> None:
        """SNES manifest should track the synthetic DSP audio golden suite."""

        manifest = load_manifest()
        asset = next(
            (asset for asset in manifest.get("assets", []) if asset.get("id") == "snes-dsp-audio-golden-windows"),
            None,
        )

        self.assertIsNotNone(asset, "expected the snes-dsp-audio-golden-windows asset")
        self.assertEqual(asset.get("suite"), "dsp_audio_golden_tests")
        self.assertEqual(asset.get("oracle_type"), "audio_sample")


if __name__ == "__main__":
    unittest.main()
