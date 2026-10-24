# nr-ayk5.4 — retrospective

- **Implementer:** Storm
- **Date:** 2026-10-24
- **PR:** #3389

## A fix for a review finding broke a failure state the acceptance had agreed

**What happened.** The first review found that the joiner's count lagged the host's because the joiner loaded the game only after `start`. To fix it I added a `prepared`/`go` handshake in which the joiner set the received game up, closing its own, on `start`, before the host's last check. The follow-up review pointed out that this let several failures (host stops its game, a stall during set-up, a failed set-up) happen after the joiner's own game was gone. The acceptance's States list says that when sending breaks off or the game is unusable, "The joiner's own game (if any) stays open." A third review round was then needed for the guards the reworked handshake still lacked (the phone's Eject, the joiner's set-up window).
**Why.** I checked the fix against the finding and its tests, not against the acceptance's list of states. A protocol step that moves a destructive action earlier changes which failures happen after it. Each state in the acceptance is a claim about every point of failure, and I did not walk them again.
**Cost.** Two extra review rounds, about 40 minutes, and one design rewritten twice.
**Prevent by.** A line in `produce-bead`, *The review loop*: when an answer to a finding changes the order of a protocol or moves a destructive step (closing, replacing, deleting), walk the acceptance's *The states* list against each failure point that now falls after it before pushing.
**Seen before.** none found

## An in-memory fake link reordered messages, so a correct session looked broken

**What happened.** A session test that changed the fake link's delay mid-run failed: the joiner's `arrived` overtook its last `got`, so the host ignored it. A WebRTC data channel created with defaults is ordered and reliable, so this could not happen for real.
**Why.** The fake scheduled each message `delay` ms after sending, so a shorter delay let a later message land first.
**Cost.** About ten minutes of debugging a non-bug.
**Prevent by.** The fake in `web/src/netplay/session_sending.test.ts` now delivers each message no earlier than the one before it. Any later fake of `PeerLink` should keep the same order guarantee.
**Seen before.** none found
