# Browser production container

Build the normal production bundle, then package it:

```sh
npm ci
bash scripts/build_web.sh
docker build --build-arg REVISION="$(git rev-parse HEAD)" -t neser-web .
docker run --rm -p 127.0.0.1:18081:8080 neser-web
REVISION="$(git rev-parse HEAD)" node deploy/smoke.mjs
```

The smoke check requires Playwright Chromium (`npx playwright install chromium`)
and reads a repository test ROM only to upload through the browser's local
file picker. It does not copy that ROM into the image.

Docker packages only `dist/index.html` and `dist/assets/`; the allowlisted build
context excludes source, credentials, `web/roms`, `web/pkg`, test ROMs and even a
`dist/roms` symlink left by the development server. It supplies an empty ROM
manifest so the player can load local files without a failed manifest request.
There is no runtime backend or server-side save storage.

Caddy serves port 8080 with `/healthz` and `/release.json`. Missing files return
404 rather than HTML. Hashed assets, including WASM with its proper MIME type,
are immutable; the HTML, identity and ROM manifest are not cached. Directory
listing and Caddy's admin API are disabled.

CI's `web-container` job runs for every main push and PR, after every other
required job succeeds or is legitimately path-skipped. It builds the normal
Rust/WASM/Vite output, tests the real production image (HTTP, WASM startup and
local ROM loading), then exports that same image to a separate main-push-only
`publish-web` job with registry-write permission. PR builds stay read-only. Tag
release workflow calls and PRs cannot publish deployment manifests.
`deployment-manifest/deployment.json` contains the full revision and immutable
`web` image digest. The VPS selects the newest successful main run, not a mutable
image tag; it may skip intermediate commits. No VPS credentials are held in CI.
Reruns retain an existing deployment manifest and its original image digest
rather than republishing different images for the same run.

Runtime definitions, routing, token/package access, initial deployment approval
and poller enrollment belong to `rmstdope-infra` (`docs/neser.md`). Deployment
is opt-in after those reviewed changes are installed. This does not modify
`scripts/deploy.py`, SFTP destinations, native release packaging or tag policy.
