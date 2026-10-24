# nr-ayk5.5 — retrospective

- **Implementer:** Cyclops
- **Date:** 2026-10-24
- **PR:** #3390

## Playwright's `toBeEnabled` reads `aria-disabled="true"` as disabled, so the shared running-state helper fails on any greyed-in-place control

**What happened.** Playing together greys Eject in place with `aria-disabled="true"` (so its tip still shows on hover and Tab), as Save and Load already were. Every two-browser test then failed in `waitForRunningState`, which waits for `#stop` to be enabled. The failure read "Received: disabled" while the button had no `disabled` attribute. I added console logging to the page to prove that `stopEnabled` was true before I found the cause.
**Why.** Playwright's enabled check honours `aria-disabled`, and `waitForRunningState` (`web/integration/helpers/lifecycle.helpers.ts`) treats an enabled Eject as part of "running".
**Cost.** One 16-minute local spec run and about 20 minutes of debugging.
**Prevent by.** A line on `waitForRunningState` and `waitForPausedState` in `lifecycle.helpers.ts` saying they assume Eject is not greyed (`aria-disabled`), and that a page which greys it needs its own wait, as `waitForPlaying` in `play-together.integration.spec.ts` does.
**Seen before.** none found

## Everything drawn over the game picture was hidden from screen readers and from `getByRole`

**What happened.** `getByRole("button", { name: "Resume" })` found nothing inside the new pause screen, although the button was in the DOM and had focus.
**Why.** `#touch-controls` in `web/index.html` carries `aria-hidden="true"` and wraps the whole picture (`#screen-wrap`), not just the touch pads. Anything drawn over the picture was therefore outside the accessibility tree, including the existing "The game stopped" alert.
**Cost.** One local spec run, plus reading a trace's aria snapshot to find it.
**Prevent by.** This PR moves `aria-hidden` onto the two touch-pad areas. A comment above `#touch-controls` now says the picture between them is for everyone.
**Seen before.** none found

## Each local run of the two-browser spec with signed Chrome lost five minutes per worker at teardown

**What happened.** With the throwaway `channel: "chrome"` config the nr-ayk5.3 retrospective calls for, every run ended with "worker-N process did not exit within 300000ms after stop, force-killed it", once per worker. Seven tests took about 16 minutes, of which roughly 5 were actual tests.
**Why.** Not established. CI's Linux Chromium shards finish in their usual 11–15 minutes, so it looks specific to signed Chrome on the macOS host.
**Cost.** About 10 minutes per local run, over six runs.
**Prevent by.** Add `--workers=1` to the local signed-Chrome run in the nr-ayk5.3 trap entry, if that turns out to shorten the teardown; otherwise note the expected 15 minutes there, so a producer plans for it.
**Seen before.** docs/retrospectives/nr-ayk5.3.md (the signed-Chrome workaround itself)
