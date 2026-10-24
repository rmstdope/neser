# nr-ayk5.3 — retrospective

- **Implementer:** Cyclops
- **Date:** 2026-10-24
- **PR:** #3388

## Two browser pages could not connect over WebRTC on the macOS host with Playwright's bundled Chromium

**What happened.** In a spike, two pages in Playwright's bundled headless Chromium swapped a non-trickle offer and answer (`iceServers: []`) and never connected: `getStats()` showed every connectivity check sent and none received, on both sides, with mDNS host candidates and with raw LAN addresses. Trickle ICE inside one page did connect, which pointed at the code rather than the machine. The same exchange connected at once with `channel: "chrome"` (signed Google Chrome), across two separate browser contexts. It also passed on CI's Linux Chromium.
**Why.** Partly established: the host's application firewall is on with stealth mode (`socketfilterfw --getglobalstate --getstealthmode`), which drops unsolicited inbound UDP to an unsigned binary such as `chrome-headless-shell`. Why trickle within one page got through was not established.
**Cost.** About 25 minutes of spike before the build plan could be written. The WebRTC spec cannot run locally on the bundled browser; I validated it with a throwaway Playwright config that sets `channel: "chrome"`.
**Prevent by.** A trap entry in `.cerebro/traps.md`: "WebRTC between Playwright pages on a macOS host with the firewall in stealth mode needs `channel: "chrome"`; the bundled headless shell's UDP is dropped. CI (Linux) is unaffected." The next netplay beads (nr-ayk5.4–.7) all run this spec.
**Seen before.** none found

## A Vitest session test that waited a fixed number of event-loop turns for a deflate stream went red on CI only

**What happened.** `session.test.ts` "starts at step 1 with the host's code to send" passed locally many times. In CI (run 40060039537, job `web`) it failed with `.toMatch() expects to receive a string, but got object`: the code was still `null` after `settle()`'s 20 `setImmediate` turns, because `CompressionStream` took longer on the runner.
**Why.** `settle()` assumed a fixed number of turns was enough for `CompressionStream("deflate-raw")` to finish. How many turns it needs depends on the machine.
**Cost.** One red CI run and one of the bead's three fix attempts.
**Prevent by.** Under fake timers, wait for the condition, bounded by a turn count, rather than a fixed count (`settle()` in `web/src/netplay/session.test.ts` now does). The same rule would fit as a line in the TDD section of `CLAUDE.md`, on async work that is not timer-driven.
**Seen before.** none found
