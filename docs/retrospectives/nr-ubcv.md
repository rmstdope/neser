# nr-ubcv — a test-only `include_str!` of a web file turned the Python gate red

## What happened
The plan embedded `web/src/console/consoles.ts` in a Rust unit test with `include_str!` (inside `#[cfg(test)]`). Every Rust leg of the gate passed, but `scripts/test_crate_package.py` (`test_package_carries_every_file_the_source_embeds`) scans every `include_str!`/`include_bytes!` in `src/` regardless of `cfg`, and requires the target to be in the published crate, which carries no `web/` files.

## Why
The package check is textual, deliberately so (nr-d2l), and it cannot tell a test-only embed from one the build needs. Nothing near the Rust tests says so.

## Cost
One full gate run (about 20 minutes) and an amendment to the plan.

## Prevent by
Read repository files that only a test needs at run time from `env!("CARGO_MANIFEST_DIR")`, as `src/nes/region.rs` and others do. A line in `.cerebro/traps.md` saying this would let the next plan get it right.

## Seen before
Not in `docs/retrospectives/`.
