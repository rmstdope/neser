# nr-ks6: two even Playwright shards did not halve the test phase

**What happened.** The bead expected splitting the `web-integration` suite into two shards to halve its test phase (target: at most 120 s per shard, against 240 s on run 36311463718). With `fullyParallel` and plain `--shard=N/2`, run 36317969389 measured shard 1 at 84.6 s (20 tests) and shard 2 at 140.8 s (19 tests), so the acceptance failed on an otherwise green PR. Weighting the split 2:1 with `PWTEST_SHARD_WEIGHTS` gave 104.8 s and 99.6 s on run 36318621916.

**Why.** Playwright sizes shards by test count, in file order, not by duration. The slow specs (palette-button, runtime-controls, save-state-flows) sort alphabetically into the second half. Separately, the suite had grown from the bead's 35 tests to 39.

**Cost.** One extra CI round of about 6 minutes, a local JSON-reporter run of about 3 minutes to get per-test durations, and a plan amendment.

**Prevent by.** Before planning a timing target for a sharded suite, take per-test durations from one run (`--reporter=json`) and simulate the split. For Playwright, weights are set with `PWTEST_SHARD_WEIGHTS` (counts, not time); the weighting is noted in `.github/workflows/ci.yml` and `architecture.md`. A bead that quotes a test count should re-count it when planning.

**Seen before.** No.
