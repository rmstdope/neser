# nr-bsr — retrospective

- **Implementer:** Storm
- **Date:** 2026-09-27
- **PR:** #3236

## `npx playwright install` hung for ever after a complete download

**What happened.** `npx playwright install --only-shell chromium` (with @playwright/test 1.58.2) printed nothing for five minutes and never finished. With `DEBUG=pw:install` and its output sent to a file rather than a pipe, the log showed that the 91 MB download finished in 12 seconds, followed by `extracting archive` and then nothing, at 0% CPU. Only 1.5 MB of the archive reached `~/Library/Caches/ms-playwright/chromium_headless_shell-1208`. Calling playwright-core's `zipBundle.extract` directly on the same zip (which `unzip -t` reports as intact) reproduced it: the promise neither resolves nor rejects, and Node exits with the event loop empty. A second session's install (nr-1kb, same version) was stuck the same way at the same time, and held the Playwright cache's `__dirlock`, so every other install on the host queued behind it.
**Why.** @playwright/test 1.58's bundled zip extraction does not work under Node 26.9, the host's node. Playwright 1.63 rewrote the extraction (`third_party/extractZip`) and installs cleanly under Node 26; the bead moved to 1.63. The nr-zdy.1 retrospective put this hang down to "no network", which this run shows was wrong: the network was fine.
**Cost.** About 25 minutes: two five-minute timeouts, then isolating the cause.
**Prevent by.** Already applied in this PR: @playwright/test 1.63, and the `install_shell` browser step bounded (`perl -e 'alarm 300'`) and non-fatal, so a stuck install cannot hold up worktree preparation. Next time Playwright or Node is bumped, run `npx playwright install --only-shell chromium` once under the host's node before merging.
**Seen before.** nr-zdy.1, misdiagnosed there as a network hang.

## wasm-pack test failed locally with "http status: 404" from ChromeDriver, again

**What happened.** The `wasm-pack test` leg of `./scripts/gate-full.sh` failed with `Error: http status: 404`. wasm-pack's cached ChromeDriver was 154.0.8037.57; the installed Chrome was 153.0.8010.53. With ChromeDriver 153.0.8010.53 from Chrome for Testing passed as `--chromedriver`, 123/123 passed.
**Why.** wasm-pack fetches the latest stable ChromeDriver, which ran ahead of the installed Chrome.
**Cost.** About 10 minutes.
**Prevent by.** As the earlier files propose. Nothing new here.
**Seen before.** nr-09s, nr-273, nr-1gg, nr-6e9, nr-aph, nr-ps1, nr-630, nr-72o, nr-nr7, nr-hab.1, nr-zdy.3, nr-qoi, nr-zdy.1.

## The Python gate legs failed in a fresh worktree: no `.venv`, and the main checkout's lacks dependencies

**What happened.** With no `.venv` in the tree, the main checkout's `.venv` gave 28 import errors and has no mypy. Creating a tree `.venv` with `--group scripts/pyproject.toml:test --group scripts/pyproject.toml:dev` made every leg green.
**Why.** `install_shell` does not create the Python environment.
**Cost.** About 5 minutes.
**Prevent by.** As the earlier files propose. Nothing new here.
**Seen before.** nr-1gg, nr-273, nr-6e9, nr-630, nr-aph, nr-nr7, nr-hab.1, nr-qoi, nr-ve3, nr-ps1, nr-zdy.3.
