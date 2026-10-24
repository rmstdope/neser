# nr-ayk5.7 — retrospective

- **Implementer:** Cyclops
- **Date:** 2026-10-24
- **PR:** #3393

## A planned drift-correction module turned out to be unneeded, and the play test harness hid the case it was for

**What happened.** The plan had a `pace` module: the side a frame or more ahead would rest a tick, so that the slower side never ran two frames at once to catch up. Its first two-sided test in `web/src/netplay/play.test.ts` (61 vs 60 fps) passed before any pace code existed. The harness's fixed 10 ms one-way delay (`ONE_WAY_MS`) keeps the faster side's lead under the slower side's catch-up threshold. At a LAN's 1 ms the test went red as intended. But the module's averaged lead stayed at ~0.5 frames under that drift, so it never rested. Working it through: lockstep with input delay D already lets the faster side lead by at most about D frames. It then either holds one frame, or the slower side runs one double frame. That is a one-frame correction, the least possible on a fixed-rate display, and resting early only moves which side makes it. The module was removed and the plan amended.

**Why.** The plan was written from the description ("correction for the two browsers running at slightly different speeds") without first measuring what lockstep plus the existing catch-up already do under drift. The harness's single latency setting made the LAN case invisible.

**Cost.** About an hour: a module, its tests, a session message type and integration, built and then taken out; a plan amendment.

**Prevent by.** For a netplay timing behaviour, write the two-sided `play.test.ts` case first, at both `oneWayMs = 1` (LAN) and `ONE_WAY_MS`, before planning a module. If it is already green, the plan needs no module. The harness now has the adjustable `oneWayMs` and `runSteady` (fractional animation-frame intervals; `run` rounds them to whole milliseconds) for exactly this.

**Seen before.** None found.
