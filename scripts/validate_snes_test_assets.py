"""Validate SNES automated-test asset provenance metadata."""

from __future__ import annotations

import json
import re
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_MANIFEST = REPO_ROOT / "roms/snes/automated_tests/manifest.json"

ALLOWED_STATUS = {"committed_ci", "optional_local"}
ALLOWED_ORACLE_TYPES = {"vector_state", "rom_pass_fail", "screen_crc", "audio_sample"}
ALLOWED_INTEGRITY_KINDS = {"tree_sha256", "not_vendored"}
HEX64_RE = re.compile(r"^[0-9a-f]{64}$")


def _is_non_empty_string(value: Any) -> bool:
    return isinstance(value, str) and bool(value.strip())


def _validate_source(asset: dict[str, Any], errors: list[str], asset_id: str) -> None:
    source = asset.get("source")
    if not isinstance(source, dict):
        errors.append(f"asset '{asset_id}': source must be an object")
        return

    for key in ("url", "ref"):
        if not _is_non_empty_string(source.get(key)):
            errors.append(f"asset '{asset_id}': source.{key} must be a non-empty string")


def _validate_integrity(
    variant: dict[str, Any], errors: list[str], asset_id: str, variant_id: str
) -> None:
    integrity = variant.get("integrity")
    prefix = f"asset '{asset_id}' variant '{variant_id}'"
    if not isinstance(integrity, dict):
        errors.append(f"{prefix}: integrity must be an object")
        return

    kind = integrity.get("kind")
    if kind not in ALLOWED_INTEGRITY_KINDS:
        errors.append(
            f"{prefix}: integrity.kind must be one of {sorted(ALLOWED_INTEGRITY_KINDS)}"
        )
        return

    sha256 = integrity.get("sha256")
    file_count = integrity.get("file_count")
    total_size = integrity.get("total_size_bytes")

    if not isinstance(file_count, int) or file_count < 0:
        errors.append(f"{prefix}: integrity.file_count must be an integer >= 0")
    if not isinstance(total_size, int) or total_size < 0:
        errors.append(f"{prefix}: integrity.total_size_bytes must be an integer >= 0")

    if kind == "tree_sha256":
        if not isinstance(sha256, str) or not HEX64_RE.fullmatch(sha256):
            errors.append(f"{prefix}: integrity.sha256 must be a lowercase 64-char hex string")
        if isinstance(file_count, int) and file_count <= 0:
            errors.append(f"{prefix}: tree_sha256 integrity requires file_count > 0")
        if isinstance(total_size, int) and total_size <= 0:
            errors.append(f"{prefix}: tree_sha256 integrity requires total_size_bytes > 0")

    if kind == "not_vendored":
        if sha256 != "not_applicable":
            errors.append(f"{prefix}: not_vendored integrity must use sha256='not_applicable'")


def validate_manifest(manifest: dict[str, Any], repo_root: Path = REPO_ROOT) -> list[str]:
    """Return a list of validation errors. Empty list means success."""

    errors: list[str] = []

    if manifest.get("schema_version") != 1:
        errors.append("schema_version must be exactly 1")

    if manifest.get("platform") != "snes":
        errors.append("platform must be 'snes'")

    assets = manifest.get("assets")
    if not isinstance(assets, list) or not assets:
        errors.append("assets must be a non-empty list")
        return errors

    seen_asset_ids: set[str] = set()

    for index, asset in enumerate(assets):
        if not isinstance(asset, dict):
            errors.append(f"assets[{index}] must be an object")
            continue

        asset_id = asset.get("id", f"assets[{index}]")
        if not _is_non_empty_string(asset.get("id")):
            errors.append(f"assets[{index}]: id must be a non-empty string")
            continue
        if asset_id in seen_asset_ids:
            errors.append(f"duplicate asset id '{asset_id}'")
            continue
        seen_asset_ids.add(asset_id)

        for required in ("suite", "platform", "license", "oracle_type", "notes"):
            if not _is_non_empty_string(asset.get(required)):
                errors.append(f"asset '{asset_id}': {required} must be a non-empty string")

        if asset.get("platform") != "snes":
            errors.append(f"asset '{asset_id}': platform must be 'snes'")

        if asset.get("oracle_type") not in ALLOWED_ORACLE_TYPES:
            errors.append(
                f"asset '{asset_id}': oracle_type must be one of {sorted(ALLOWED_ORACLE_TYPES)}"
            )

        _validate_source(asset, errors, asset_id)

        variants = asset.get("variants")
        if not isinstance(variants, list) or not variants:
            errors.append(f"asset '{asset_id}': variants must be a non-empty list")
            continue

        seen_variant_ids: set[str] = set()
        for variant_index, variant in enumerate(variants):
            if not isinstance(variant, dict):
                errors.append(f"asset '{asset_id}': variants[{variant_index}] must be an object")
                continue

            variant_id = variant.get("id", f"variants[{variant_index}]")
            if not _is_non_empty_string(variant.get("id")):
                errors.append(
                    f"asset '{asset_id}': variants[{variant_index}].id must be a non-empty string"
                )
                continue
            if variant_id in seen_variant_ids:
                errors.append(f"asset '{asset_id}': duplicate variant id '{variant_id}'")
                continue
            seen_variant_ids.add(variant_id)

            if variant.get("status") not in ALLOWED_STATUS:
                errors.append(
                    f"asset '{asset_id}' variant '{variant_id}': status must be one of {sorted(ALLOWED_STATUS)}"
                )

            if not _is_non_empty_string(variant.get("path")):
                errors.append(f"asset '{asset_id}' variant '{variant_id}': path must be non-empty")

            if not _is_non_empty_string(variant.get("notes")):
                errors.append(f"asset '{asset_id}' variant '{variant_id}': notes must be non-empty")

            refresh_command = variant.get("refresh_command")
            if variant.get("status") == "optional_local" and not _is_non_empty_string(refresh_command):
                errors.append(
                    f"asset '{asset_id}' variant '{variant_id}': optional_local variants require refresh_command"
                )

            _validate_integrity(variant, errors, asset_id, variant_id)

            if variant.get("status") == "committed_ci" and _is_non_empty_string(variant.get("path")):
                variant_dir = repo_root / str(variant["path"])
                if not variant_dir.exists() or not variant_dir.is_dir():
                    errors.append(
                        f"asset '{asset_id}' variant '{variant_id}': committed_ci path does not exist: {variant['path']}"
                    )

    return errors


def load_manifest(path: Path = DEFAULT_MANIFEST) -> dict[str, Any]:
    """Load and parse the manifest JSON file."""

    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def main() -> int:
    """Validate the default SNES manifest. Return process exit code."""

    try:
        manifest = load_manifest(DEFAULT_MANIFEST)
    except FileNotFoundError:
        print(f"error: manifest not found: {DEFAULT_MANIFEST}")
        return 1
    except json.JSONDecodeError as exc:
        print(f"error: invalid JSON in manifest: {exc}")
        return 1

    errors = validate_manifest(manifest)
    if errors:
        print("SNES test-asset manifest validation failed:")
        for error in errors:
            print(f"- {error}")
        return 1

    print("SNES test-asset manifest validation passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
