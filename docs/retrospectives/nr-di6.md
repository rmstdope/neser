# nr-di6 — retrospective

- **Implementer:** Storm
- **Date:** 2026-09-28
- **PR:** #3290

## A Playwright spec meant to fail passed, because its route matched no request

**What happened.** To reproduce review finding 1 (a ROM chosen before the wasm loads is refused),
the new `app-shell` spec held the wasm download with `page.route("**/neser_bg.wasm", …)`. It passed
against the unfixed code. Widening the pattern to `**/*.wasm` turned it red, as the finding
predicted.

**Why.** Playwright's server runs `scripts/build_web.sh`, a Vite build, and Vite renames
`web/pkg/neser_bg.wasm` to a content-hashed asset (`neser_bg-<hash>.wasm`). The pattern written from
`app.ts`'s `new URL("../pkg/neser_bg.wasm", …)` never matched, and an unmatched `page.route`
fails silently.

**Cost.** One extra Playwright run, about five minutes. Without the RED-first habit the spec would
have shipped as a test that proves nothing.

**Prevent by.** In the web integration specs, match built assets by extension or a hash-tolerant
glob (`**/*.wasm`, `**/neser_bg*.wasm`), never by the source file name. Better, a spec that routes a
request asserts that its handler ran, so a pattern that matches nothing fails.

**Seen before.** None found.
