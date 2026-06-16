# 65816 ProcessorTests subset (pinned)

This directory contains a pinned subset of Tom Harte's ProcessorTests `65816/v1` vectors.

Source repository: `https://github.com/SingleStepTests/ProcessorTests`

Pinned upstream commit: `bb11756436da8fd16cce86aef63dc6725f48836f`

Included files:

- `v1/00.e.json` (first 32 vectors)
- `v1/00.n.json` (first 32 vectors)
- `v1/ea.e.json` (first 32 vectors)
- `v1/ea.n.json` (first 32 vectors)

These files are used by ignored SNES integration tests in `src/snes/integration_tests/processor_tests_65816.rs`.

To refresh this subset from upstream, run:

`bash scripts/refresh_65816_processor_tests_subset.sh`
