# 65816 ProcessorTests (pinned)

This directory contains a pinned subset of Tom Harte's ProcessorTests `65816/v1` vectors for git tracking.

Source repository: `https://github.com/SingleStepTests/ProcessorTests`

Pinned upstream commit: `bb11756436da8fd16cce86aef63dc6725f48836f`

Tracked subset files:

- `v1/00.e.json`
- `v1/00.n.json`
- `v1/ea.e.json`
- `v1/ea.n.json`

Optional full corpus location (local, ignored by git):

- `full/v1/*.json`

SNES integration tests in `src/snes/integration_tests/processor_tests_65816.rs` use the tracked subset by default and automatically switch to full-corpus files (per filename) when `full/v1` exists.

To refresh the local full corpus from upstream, run:

`bash scripts/refresh_65816_processor_tests_subset.sh`
