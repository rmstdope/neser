# nr-9qg — retrospective

- **Implementer:** Cyclops
- **Date:** 2026-09-27
- **PR:** #3279

## The local gate and `test-dir.sh` run none of the native frontend's tests

**What happened.** The plan validated the desktop change with
`./scripts/test-dir.sh src/frontends/native`. It reported `0 passed; ... 13731 filtered out`.
`./scripts/gate-full.sh` passed, but its two unit-test legs (`cargo test --no-default-features
--lib`, lines 45 and 61) never compile `src/frontends/native`, so "full gate passed" proved
nothing about this bead's Rust tests (`sight_picture_*`, `crosshair_returns_position_with_zapper`,
the Super Scope sight tests). They pass under `cargo test --lib frontends::native` (default
features: 428 tests). CI does run them, through `cargo nextest archive --lib --all-features` in
`.github/workflows/ci.yml`, so the gap is local only: a native-frontend regression passes the
pre-PR gate and turns red only in CI.

**Why.** `scripts/test-dir.sh` defaults `CARGO_TEST_ARGS` to `--no-default-features`, and
`gate-full.sh` runs the same configuration. The `native` feature, which the frontend module
needs, is then off.

**Cost.** Little here. It was noticed because the zero count looked wrong. The plan's validation
had to be amended mid-build.

**Prevent by.** Add a `cargo test --lib frontends::native` leg to `scripts/gate-full.sh`, or run
the lib tests with `--all-features` as CI does. Have `test-dir.sh` warn when a requested
directory matches no tests under its feature set, instead of silently reporting zero.

**Seen before.** None found.
