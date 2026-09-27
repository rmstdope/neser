# nr-gtx — retrospective

- **Implementer:** Storm
- **Date:** 2026-09-27
- **PR:** #3283

## Playwright's webServer timed out on a fresh worktree's first wasm build

**What happened.** The first `smoke-port -- npx playwright test web/integration/specs/snes-filter.integration.spec.ts` in the freshly prepared worktree ended with `Error: Timed out waiting 600000ms from config.webServer.` No test ran. After a separate `bash scripts/build_web.sh` had finished (it took several more minutes), the same command built again incrementally and the specs ran in 1–4 minutes.
**Why.** Not fully established. `playwright.config.ts` says a cold worktree needs "about two minutes of cargo alone" and allows 600 s. Here the release wasm build from an empty `target/` took longer than 10 minutes, with the host Rust build of the same tree competing for the CPU.
**Cost.** About 15 minutes: one wasted 10-minute run, then the separate warm-up build.
**Prevent by.** Build the web bundle as part of worktree preparation. That means `.cerebro/project.conf`'s `prewarm` running `bash scripts/build_web.sh` for beads whose plan names a Playwright spec. Alternatively, `playwright.config.ts`'s `webServer.timeout` could be sized to a measured cold build, not two minutes.
**Seen before.** nr-zdy.1 (a webServer timeout while building the wasm, with a different cause: `wasm-bindgen` missing from PATH).
