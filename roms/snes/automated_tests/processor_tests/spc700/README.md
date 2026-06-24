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
- `v1/e5.json`
- `v1/c5.json`
- `v1/f5.json`
- `v1/f6.json`
- `v1/d5.json`
- `v1/d6.json`
- `v1/f8.json`
- `v1/eb.json`
- `v1/e9.json`
- `v1/ec.json`
- `v1/f9.json`
- `v1/fb.json`
- `v1/c9.json`
- `v1/cc.json`
- `v1/e7.json`
- `v1/f7.json`
- `v1/c7.json`
- `v1/d7.json`
- `v1/7c.json`
- `v1/3c.json`
- `v1/24.json`
- `v1/04.json`
- `v1/44.json`
- `v1/88.json`
- `v1/84.json`
- `v1/a8.json`
- `v1/a4.json`
- `v1/68.json`
- `v1/c8.json`
- `v1/c0.json`
- `v1/ad.json`
- `v1/2f.json` (BRA)
- `v1/f0.json` (BEQ)
- `v1/d0.json` (BNE)
- `v1/b0.json` (BCS)
- `v1/90.json` (BCC)
- `v1/70.json` (BVS)
- `v1/50.json` (BVC)
- `v1/30.json` (BMI)
- `v1/10.json` (BPL)

Optional full corpus location (local, ignored by git):

- `full/v1/*.json`

SNES integration tests in `src/snes/integration_tests/processor_tests_spc700.rs` use the tracked subset by default and automatically prefer full-corpus files (per filename) when `full/v1` exists.
