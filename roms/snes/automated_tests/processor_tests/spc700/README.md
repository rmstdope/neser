# SPC700 ProcessorTests (pinned)

This directory contains a pinned subset of Tom Harte's ProcessorTests `spc700/v1` vectors for git tracking.

Source repository: `https://github.com/SingleStepTests/ProcessorTests`

Tracked subset files:

- `v1/00.json`
- `v1/fd.json`
- `v1/10.json` (BPL)
- `v1/4d.json`
- `v1/3d.json`
- `v1/5c.json`
- `v1/60.json`
- `v1/4b.json`

Optional full corpus location (local, ignored by git):

- `full/v1/*.json`

SNES integration tests in `src/snes/integration_tests/processor_tests_spc700.rs` use the tracked subset by default.

Refresh workflow:

- `bash scripts/refresh_spc700_processor_tests_subset.sh` fetches local full corpus (`full/v1`).
- `python -m scripts.refresh_spc700_processor_tests_subset` regenerates the tracked subset and `subset_coverage_report.json` deterministically.

When both subset and full corpus exist locally, the integration tests use committed subset files; full corpus is only used as a fallback when the subset directory is absent.
