# nr-09s — retrospective

- **Implementer:** Storm
- **Date:** 2026-09-25
- **PR:** #3189

## The new rust-toolchain.toml was ignored in the fleet session

**What happened.** After adding `rust-toolchain.toml`, `rustup show active-toolchain` in the worktree printed `stable-aarch64-apple-darwin (overridden by environment variable RUSTUP_TOOLCHAIN)`. Every fleet session had inherited `RUSTUP_TOOLCHAIN=stable-aarch64-apple-darwin`.
**Why.** rustup's `cargo` proxy exports `RUSTUP_TOOLCHAIN` to every process it starts, and the environment variable beats a toolchain file. The fleet view is started through `cargo run`, so every session it launches, and everything those sessions run, inherits `stable`. Stable happens to equal the 1.98.1 pin today; after the next stable release, fleet sessions would silently have linted and formatted with a different toolchain than CI.
**Cost.** About ten minutes to find the source. The fix grew the PR to unset the variable in `scripts/gate-full.sh`, `scripts/test-dir.sh` and `.githooks/pre-commit`, which review finding 2 also asked for.
**Prevent by.** Removing the variable where it starts: Cerebro's launcher (`.cerebro/cerebro/scripts/launch`, or the fleet view's session spawner) should `unset RUSTUP_TOOLCHAIN` (and `CARGO`, `RUSTC` and similar per-process variables cargo exports) before starting a session. Until then, ad-hoc `cargo` commands in a fleet session still run on `stable`.
**Seen before.** None found.

## wasm-pack test failed locally with "http status: 404" from ChromeDriver

**What happened.** The `wasm-pack test --headless --chrome` leg of `scripts/gate-full.sh` failed twice with `driver status: signal: 9 (SIGKILL)` and `Error: http status: 404`, on code whose host clippy and tests were green.
**Why.** wasm-pack downloaded the latest ChromeDriver (154.0.8037.57) into `~/Library/Caches/.wasm-pack/`, but the installed Google Chrome was 153.0.8010.53. A mismatched driver cannot create a session. This is local to the machine, not the diff.
**Cost.** About ten minutes and one full rerun of the wasm leg. It passed (94 tests) with a ChromeDriver 153 from Chrome for Testing passed through `wasm-pack test --chromedriver <path>`.
**Prevent by.** `scripts/gate-full.sh` could pass `--chromedriver` pointing at a driver that matches the installed Chrome (for example one resolved with `npx @puppeteer/browsers install chromedriver@<chrome version>`), or check `chromedriver --version` against the Chrome version and fail with a clear message instead of an HTTP 404.
**Seen before.** None found.
