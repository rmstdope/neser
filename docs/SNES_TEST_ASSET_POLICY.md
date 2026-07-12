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
- `refresh_command` for `optional_local` variants
- Variant `notes`

## CI Subset vs Optional Local Corpus

- `committed_ci` assets are versioned in Git and run in CI.
- `optional_local` assets are intentionally not vendored and should be materialized locally with the documented refresh command.
- For SPC700 ProcessorTests, use a two-step local refresh flow: first fetch the
  optional full corpus, then run
  `python -m scripts.refresh_spc700_processor_tests_subset` to regenerate the
  committed subset and coverage report deterministically.

## Baseline Approval Policy

Visual and audio baselines must follow an explicit approval workflow:

- Generated screenshots or audio captures are review artifacts by default.
- Compact committed metadata (CRC or sample windows) is added only after review approval.
- Large generated artifacts are not committed unless a separate explicit decision is made.

## Synthetic In-Code Audio Assets

The `dsp_audio_golden_tests` suite (oracle type `audio_sample`) uses fixtures
generated entirely in Rust test code, so it has no vendored files:

- The manifest entry's `committed_ci` variant points at the source directory
  (`src/snes/integration_tests`) where the fixtures and approved CRC32
  baselines live.
- Baselines are per-window CRC32 metadata (sample rate, warmup/window
  lengths, fixture source description, review note) inline in the test
  source, recorded only after navigator review of the generated WAV.
- WAV review artifacts are produced on demand with `NESER_CAPTURE_AUDIO=1`
  under `target/snes_test_captures/dsp_audio_golden_tests/` and are never
  committed.

## Review Gate

Changes to SNES automated-test assets should include:

- Manifest updates in `roms/snes/automated_tests/manifest.json`
- Passing validator tests (`python -m unittest discover -s scripts -t scripts -p "test_*.py"`)
- Updated source/provenance notes when asset intake changes
- `processor_tests` source refs that remain immutable and consistent for shared upstream URLs

## Future Expansion

When new SNES suites are added (PPU/APU/DMA/input), they must be represented in the same manifest format before they are enabled in CI.
