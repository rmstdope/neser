# SPC700 ProcessorTests (pinned)

This directory contains a pinned subset of Tom Harte's ProcessorTests `spc700/v1` vectors for git tracking.

Source repository: `https://github.com/SingleStepTests/ProcessorTests`

Tracked subset files:

- `v1/00.json`
- `v1/8d.json`
- `v1/cd.json`
- `v1/e8.json`
- `v1/7d.json`
- `v1/fd.json`
- `v1/9d.json`
- `v1/bd.json`
- `v1/e4.json`
- `v1/c4.json`
- `v1/d8.json`
- `v1/cb.json`
- `v1/e6.json`
- `v1/bf.json`
- `v1/c6.json`
- `v1/af.json`
- `v1/f4.json`
- `v1/d4.json`
- `v1/db.json`
- `v1/d9.json`

Optional full corpus location (local, ignored by git):

- `full/v1/*.json`

SNES integration tests in `src/snes/integration_tests/processor_tests_spc700.rs` use the tracked subset by default and automatically prefer full-corpus files (per filename) when `full/v1` exists.
