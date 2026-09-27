# nr-dv5 — retrospective

- **Implementer:** Bishop (driven by hand by the navigator)
- **Date:** 2026-09-27
- **PR:** #3246

## The web integration suite was red on CI and green on every local run

**What happened.** `save-state-snes`, `snes-frontend-flow` and the zoom spec in
`runtime-controls` failed on CI from #3237 on, with Playwright reporting the sidebar
`<aside>` intercepting clicks on its own buttons. The same specs passed locally, under 8x CPU
throttling too, and `document.elementsFromPoint` at the click point returned the button on CI
as well. The real cause was pointer lock: choosing a ROM file requests it on the canvas
(#855, nr-yvv), CI's Linux Chromium grants it without a gesture, and the macOS headless shell
refuses it. With the mouse captured every mouse event goes to the canvas at frozen
coordinates, and Playwright's hit-target check names whatever sits there, the sidebar. The
specs only started clicking for real in nr-1kb (#3238), which is why the capture first bit
then; the bead's own diagnosis (a layout overlay) was wrong.
**Why.** Pointer lock is granted or refused by the browser build, not by the page, so the
failure could not exist locally. Nothing in the job kept a trace, so three CI cycles went on a
temporary diagnostic spec before the mechanism was visible.
**Cost.** About three hours and five CI cycles (one to upload traces, three diagnostic, one
red-before run).
**Prevent by.** `.github/workflows/ci.yml` now uploads `test-results/` when `web-integration`
fails, so the next red run is read from its trace and error context, not from a diagnostic
spec. `waitForRunningState` in `web/integration/helpers/lifecycle.helpers.ts` releases the
captured mouse, and its doc comment says why; a spec that needs the lock held clicks the game
afterwards.
**Seen before.** nr-yvv (the automated Chrome cannot exercise pointer lock: the same browser
difference, seen from the other side).

## A per-click timeout in a spec turned a product stall into a flake

**What happened.** The zoom spec's `Zoom -` click timed out at its own 5 s limit on CI while
the same click took 4.97 s in a diagnostic run: the zoom probe sets the canvas backing store
once per candidate size, and each assignment reallocates the WebGL drawing buffer, seconds of
main thread on software GL.
**Why.** The 5 s cap was arbitrary and sat exactly on the cost of the click under CI's
renderer, so a real product cost showed up as a random failure.
**Cost.** One diagnostic CI cycle, and the flake was mixed into the pointer-lock failures
above for most of the investigation.
**Prevent by.** The spec now uses an explicit 20 s cap with a comment naming the measurement
and nr-b5h, which owns removing the reallocations. A spec timeout that is not derived from a
measurement should say so, or use the test's own.
**Seen before.** none found.
