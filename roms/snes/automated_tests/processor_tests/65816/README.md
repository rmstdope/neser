# 65816 ProcessorTests (pinned)

This directory contains a pinned mirror of Tom Harte's ProcessorTests `65816/v1` vectors.

Source repository: `https://github.com/SingleStepTests/ProcessorTests`

Pinned upstream commit: `bb11756436da8fd16cce86aef63dc6725f48836f`

All `v1/*.json` files in this directory are consumed by SNES integration tests in `src/snes/integration_tests/processor_tests_65816.rs`.

To refresh this corpus from upstream, run:

`bash scripts/refresh_65816_processor_tests_subset.sh`
