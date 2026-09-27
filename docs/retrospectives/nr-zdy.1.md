# nr-zdy.1 — retrospective

- **Implementer:** Wolverine
- **Date:** 2026-09-27
- **PR:** #3228

## wasm-pack test failed locally with "http status: 404" from ChromeDriver, again

**What happened.** `./scripts/gate-full.sh` stopped at `wasm-pack test --headless --chrome`: `driver status: signal: 9 (SIGKILL)`, then `Error: http status: 404`. The rerun outside the sandbox failed the same way. `--safari` failed too, with `driver failed to bind port during startup`. The legs after it had to be run by hand, and the three new wasm tests ran only in CI.
**Why.** Google Chrome here is 153.0.8010.53 and wasm-pack's cached ChromeDriver is 154.0.8037.57. The session had no DNS (`curl: (6) Could not resolve host: storage.googleapis.com`), so no matching driver could be fetched.
**Cost.** About 20 minutes, and a gate leg proven only by CI.
**Prevent by.** The fleet host keeping Chrome and the wasm-pack ChromeDriver on the same major version, or the `install` step in `.cerebro/project.conf` pinning a driver that matches the installed Chrome.
**Seen before.** nr-09s, nr-1gg, nr-273, nr-630, nr-6e9, nr-72o, nr-aph, nr-hab.1, nr-nr7, nr-ps1, nr-qoi, nr-ve3.

## Playwright could not run as configured: port 8000 taken, and the browser download hung

**What happened.** The Playwright `webServer` first timed out while it built the wasm, because `wasm-bindgen` was not on PATH. It exists only under `~/Library/Caches/.wasm-pack/wasm-bindgen-cargo-install-0.2.108/`. Once the build worked, the server still timed out. An unrelated `omlx-server` process, four days old, was listening on 127.0.0.1:8000. `npx playwright install chromium-headless-shell` then hung for more than 20 minutes at 0% CPU, since there was no network. What worked in the end: a throwaway config serving `dist/` on port 8317, with `executablePath` pointed at the cached `chromium_headless_shell-1243`.
**Why.** `playwright.config.ts` hardcodes port 8000 and the `@playwright/test` browser revision. The machine had a different revision cached and no network.
**Cost.** About 40 minutes.
**Prevent by.** `playwright.config.ts` reading its port from the variable `smoke-port` exports, with a `port_base` declared in `.cerebro/project.conf`. `scripts/build_web.sh` falling back to wasm-pack's cached `wasm-bindgen`.
**Seen before.** nr-1gg.
