# nr-0pe — retrospective

- **Implementer:** Cyclops
- **Date:** 2026-09-28
- **PR:** #3294

## The new GBA looks drew nothing, and every test was green

**What happened.** With the four GBA looks wired in, the unit tests and the first integration spec (button words, no page errors, no `console.error` about shaders) all passed. A headless screenshot of each look then showed AGB-001, Switch Online, GBA SP and LCD Grid identical to None. Logging `gl.getError()` around each draw gave `1282` (INVALID_OPERATION) on every pass: `WebGL: INVALID_OPERATION: drawArrays: no buffer is bound to enabled attribute`.
**Why.** WebGL 1 refuses a draw while *any* enabled vertex attribute array has no live buffer, even one the current program does not read. The existing pipelines leave `a_texCoord` enabled, and `initWebGL` (run on every look change) deletes that buffer and makes a new one, so the array points at a deleted buffer. The existing single-pass, NTSC and Game Boy pipelines never notice because they re-bind `a_texCoord` every frame. Chrome reports the refusal only as a console *warning*, which the spec's error collector ignored.
**Cost.** About an hour: the picture had to be looked at before the failure was even visible, then located with per-pass `getError` logging. It would have shipped as a Filter button that changes words only, which is the bug this bead was filed for.
**Prevent by.** A new WebGL pass in `web/src/` disables every attribute array it does not use before drawing (as `display/gba_pipeline.ts` `pass()` now does), and a web spec that adds a look collects console messages matching `INVALID_` alongside page errors and probes the picture itself (screenshot decoded in the page, as `pictureAt` in `web/integration/specs/gba-filter.integration.spec.ts`), not just the button words.
**Seen before.** None found.

## The picture probes passed locally and timed out on CI, twice

**What happened.** The new picture tests wait for `ppu/shades.gba` to show its blue bands. On CI `web-integration (1/2)` they timed out at that wait, first with one test, then (after moving the probe) with all three. The first trace showed the game running, but the probe sat at x = 0.5 on a band of b ≈ 57, under the `b > 60` threshold, and which band a fixed fraction lands on moves with the canvas size. The second trace's canvas screenshot still showed the NESER boot logo after 20 s.
**Why.** Two causes. (1) The probe point was too close to the threshold. (2) The web always plays the GBA BIOS intro, about 290 frames: 4.8 s at 60 fps locally, 13 s at 6× CPU throttle, and longer than 20 s on a CI runner running several GBA games at once.
**Cost.** Two of the bead's three red-CI fix attempts, and about 40 minutes of CI.
**Prevent by.** A web spec that probes a GBA game's picture budgets for the intro (`test.slow()` and a wait of about 100 s), and it measures the wait locally with `Emulation.setCPUThrottlingRate` at 6× before pushing. Probe points sit well inside a colour band, never on the threshold.
**Seen before.** None found.

## A 1 KB look-up table was never fetched, so the fetch-failure test could not fail

**What happened.** The spec aborted the GBA SP look-up table's request with `page.route` to exercise "the look cannot be fetched", but the button never went back. The route never fired.
**Why.** Vite inlines assets under `assetsInlineLimit` (4 KB) into the bundle as `data:` URLs. `gba-lut-64.png` is 1 KB, so it is never requested. Only the 376 KB console art is fetched.
**Cost.** One confused spec run.
**Prevent by.** A spec that routes an asset request first checks that the file is over 4 KB, or asserts that the route was hit.
**Seen before.** None found.
