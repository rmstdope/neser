# nr-fli1.1: the web frontend moves from npm and bash scripts to pnpm

## pnpm forwards a `--` that npm dropped, so both Playwright shards ran everything

**What happened.** The CI step `npm run test:integration:web -- --shard=${{ matrix.shard }}/2` was
translated word for word to `pnpm run test:integration:web -- --shard=...`. npm swallows the `--`;
pnpm 11 passes it on, and Playwright reads everything after a `--` as a test-file filter, so
`--shard` was silently ignored and each shard would have run all 118 tests. The test pinning the
step asserted the exact string, `--` included, so it passed against the defect. The cold review
caught it before merge.

**Why.** npm and pnpm differ on `--` handling for `run`; the translation was mechanical, and the
existing test pinned text rather than behaviour.

**Cost.** One review round and one fix commit; nothing reached main.

**Prevent by.** `scripts/test_web_integration_ci.py`
`test_no_step_hands_pnpm_a_bare_double_dash` (added in this PR) fails on any CI step that passes
`--` to a pnpm script. Anyone writing `pnpm run <script> <args>` passes the arguments directly.

**Seen before.** No.

## web-integration shard 1 is red on main's own bundle

**What happened.** Shard 1 failed on `app-shell.integration.spec.ts:304` (the DSP firmware-cancel
note), with 11 flaky tests, on a bundle (`index-D7X4YYQj.js`) byte-identical to the one main run
f3ca6ee2 passed with. The same test failed on main in run 40029626370, and main had other red
web-integration runs the same day. Locally, `app-shell` under `--repeat-each=3` failed a different
set of tests each run, and `gb-hardware-choice` tests take 37–44 s against the 45 s timeout when
run alone. A bare re-run went green.

**Why.** Not established. Tests near the per-test timeout under full-suite load are the likely
common factor.

**Cost.** About 40 minutes: one red shard, a local reproduction, a bundle comparison and a re-run.

**Prevent by.** A bead to make shard 1's slow specs fit their timeout (or raise `timeout` in
`playwright.config.ts` for them). Until then, compare a red shard's `assets/index-*.js` hash
against a green main run's before suspecting the change.

**Seen before.** `nr-dv5.md` (a per-click timeout turning a stall into a flake).
