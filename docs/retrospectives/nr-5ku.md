# nr-5ku — retrospective

## Every cargo call rebuilt the whole crate, because build.rs watched a missing directory

**What happened.** The plan had `test-dir.sh` list each directory's tests before running them,
assuming the list reused the run's build. Two identical `cargo test --all-features --lib` calls in a
row each printed `Compiling neser` (2.5–7 min each). `CARGO_LOG=cargo::core::compiler::fingerprint=info`
named the cause: `stale: missing ".../roms/games/mappers"`. `build.rs` emitted
`cargo:rerun-if-changed=roms/games/mappers` unconditionally. That directory is gitignored
(`roms/games`) and absent from the main checkout and every worktree. Cargo counts a missing watched
path as changed, so it reran the build script and recompiled the crate on **every** invocation.
Every gate run paid this: the fast and full unit-test legs rebuilt between each other.

**Why.** A `rerun-if-changed` hint on an optional, gitignored path. Cargo's documented behaviour for a
missing path is "always stale", and nothing surfaced it: it looks like an ordinary slow build.

**Cost.** About 15 minutes of rebuilds before it was noticed, and a plan amendment. Before the fix,
every producer's gate carried at least one needless full rebuild.

**Prevent by.** `build.rs` now watches the directory only when it exists
(`scripts/test_build_rs_rerun.py` pins that no hint names a missing path). When a build is slower
than expected, or recompiles with no source change, run the second cargo call under
`CARGO_LOG=cargo::core::compiler::fingerprint=info` and read the `stale:` line before assuming the
build is simply slow.

**Seen before.** None found.
