# nr-39ib: retrospective

## Chromium aborts an unfinished IndexedDB write when the page goes, `commit()` or not

**What happened.** The bead's lead was "the reload can beat the thumbnail write", with the choice of whether the spec or the app should wait left open. The spec flaked once in 60 CI runs and passed 30 of 30 locally, even under 20× CDP CPU throttling and with a synchronous `location.reload()` straight after the write. It went red only when a second tab held the store busy, so that the write was still waiting at the reload. A probe then showed that a get-then-put, a plain put, and a put plus `tx.commit()` are **all** lost in that case.

**Why.** On document teardown Chromium force-closes the page's IndexedDB connection and aborts every transaction that has not committed. An explicit `commit()` does not hand the write over to the backend.

**Cost.** About an hour of probing before the reproduction was deterministic. Rearranging the IndexedDB calls would have looked like a fix and changed nothing.

**Prevent by.** Anything the web app must keep from a `visibilitychange`/`pagehide`/`beforeunload` handler goes to synchronous storage (`localStorage`) first. `keepThumbnail` in `web/src/rom/recent_games.ts` is the pattern. To reproduce a "the page went before the write" race, hold the store busy from a second tab until the test releases it, as `recent-games.integration.spec.ts` now does. Throttling does not reproduce it.

**Seen before.** No.

## `vite build` under a live, reused test server turns every web spec red

**What happened.** After `npx vite build`, every spec failed in both tests, the untouched one included. A rerun was green.

**Why.** Not fully established: the previous run's `python3 -m http.server` (`reuseExistingServer: true`) most likely still had its working directory in the `dist/` that `vite build` had just deleted and recreated, so it served nothing.

**Cost.** One confusing red run that looked like a regression.

**Prevent by.** Stop the test server before rebuilding `dist/`, or rebuild only through `pnpm run build` without `NESER_WEB_PREBUILT=1`. A note under "Running tests" in `architecture.md`'s web section would carry it.

**Seen before.** `docs/retrospectives/nr-1gg.md` (`reuseExistingServer` serving another session's build).
