# Retrospective: the container smoke test went red unnoticed (Steel Glow, step 4)

## What happened

#3355 made the library the web page's start screen, so the picture (`#screen`) stays hidden until
a game runs. `deploy/smoke.mjs`, which CI's `web-container` job runs against the production image,
opened the page and expected `#screen` to be visible. It failed, and #3355 was merged anyway.
Main stayed red on `web-container`, and the image push and deploy that it gates did not run.

## Why it was missed

- **The job runs last.** `web-container` needs every other job, so it starts only after the rest
  have finished. My wait loop stopped as soon as no listed check was pending. The container job
  had not been listed yet, so the loop ended before it started.
- **The check is not required.** Branch protection requires only `rust-lint` and `wasm`, so GitHub
  let the merge through.
- **Neither local suite covers it.** The full gate and the Playwright suite never run
  `deploy/smoke.mjs`, and it needs Docker, which the producer's machine lacks.

## What changes

- **Before merging:** read `gh pr checks <n>` in full. Treat a merge as allowed only when no check
  is pending and none has failed, with `web-container` present when its job is triggered. A short
  list means some checks have not started yet.
- **When the start screen or the first view changes:** also update `deploy/smoke.mjs`. Without
  Docker, its browser steps can still be run against `scripts/build_web.sh` plus
  `scripts/run_web.sh`.
