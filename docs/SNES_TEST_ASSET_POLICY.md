# SNES Test Asset Intake and Provenance Policy

This document defines how SNES automated-test assets are tracked, reviewed, and validated.

## Goals

- Keep every committed SNES test asset auditable.
- Separate committed CI subsets from optional local corpora.
- Enforce consistent provenance metadata before new assets are accepted.

## Canonical Manifest

- Path: `roms/snes/automated_tests/manifest.json`
- Scope (v1): existing SNES ProcessorTests assets (65816 and SPC700) and their optional local full-corpus variants.
- Validation entry point: `python -m scripts.validate_snes_test_assets`

## Required Metadata

Each asset family entry must define:

- Stable `id`, `suite`, `platform`
- `source.url` and `source.ref`
- `license` status (for example known license identifier or `unknown`)
- `oracle_type` (for example `vector_state`)
- Human-readable `notes`
- One or more `variants`

Additional rule for SNES `processor_tests` assets:

- `source.ref` must be a pinned 40-character lowercase Git commit SHA (not a moving branch name)
- Assets that share the same `source.url` must also share the same `source.ref`

Each variant must define:

- Stable `id` (for example `ci_subset`, `optional_full_corpus`)
- `status` (`committed_ci` or `optional_local`)
- Repository-relative `path`
- `integrity` object
- `refresh_command` for `optional_local` variants
- Variant `notes`

## Integrity Model

v1 supports two integrity strategies:

- `tree_sha256`: for committed CI assets present in the repository.
  - Includes a deterministic SHA-256 hash over sorted per-file hash records.
  - Includes `file_count` and `total_size_bytes` for quick review sanity.
- `not_vendored`: for optional local corpora that are intentionally not committed.
  - Uses `sha256 = not_applicable` with zero file count and size in the committed manifest.

## CI Subset vs Optional Local Corpus

- `committed_ci` assets are versioned in Git and run in CI.
- `optional_local` assets are intentionally not vendored and should be materialized locally with the documented refresh command.

## Baseline Approval Policy

Visual and audio baselines must follow an explicit approval workflow:

- Generated screenshots or audio captures are review artifacts by default.
- Compact committed metadata (CRC or sample windows) is added only after review approval.
- Large generated artifacts are not committed unless a separate explicit decision is made.

## Review Gate

Changes to SNES automated-test assets should include:

- Manifest updates in `roms/snes/automated_tests/manifest.json`
- Passing validator tests (`python -m unittest discover -s scripts -t scripts -p "test_*.py"`)
- Updated source/provenance notes when asset intake changes
- `processor_tests` source refs that remain immutable and consistent for shared upstream URLs

## Future Expansion

When new SNES suites are added (PPU/APU/DMA/input), they must be represented in the same manifest format before they are enabled in CI.
