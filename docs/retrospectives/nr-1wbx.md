# nr-1wbx — retrospective

- **Implementer:** Cyclops
- **Date:** 2026-10-30
- **PR:** #3401

## A Playwright test offering a dead TURN relay passed locally and timed out tearing down on CI

**What happened.** The new play-together spec "Given liaison offers a relay, … then they connect directly and nothing says Relayed" had liaison offer `turn:127.0.0.1:9?transport=udp`, a relay nobody answers on. Locally it passed in 18 s. In CI (run 41091902968, job `web-integration (2/2)`) the test body passed, then failed twice with `Tearing down "context" exceeded the test timeout of 45000ms.`
**Why.** Chrome keeps retrying a TURN allocation over UDP to a port that never answers, and closing the browser context waits for that. Over `transport=tcp` the closed port refuses the connection at once. Locally the test then took 9 s instead of 18 s, and CI went green on the next head. Why the UDP wait outlasted 45 s only on the runner was not established: the runner may not get ICMP "port unreachable" back as quickly as macOS does.
**Cost.** One CI cycle (about 20 minutes) and one of the bead's three fix attempts.
**Prevent by.** A test that hands the browser an ICE server that must fail uses `transport=tcp` to a closed port, never UDP. The comment above `liaison.relay` in `web/integration/specs/play-together.integration.spec.ts` says why. That rule would fit as a line under the Playwright notes in `web/integration/` or in `architecture.md`'s testing strategy.
**Seen before.** none found.

## An unrelated app-shell test failed on both CI attempts and passed 5 of 5 locally

**What happened.** On head 95e79b8, `web-integration (1/2)` failed with `app-shell.integration.spec.ts:304` "Given a DSP game whose firmware dialog is cancelled, …". The `Not started: … needs the DSP-1 firmware` toast was not visible within 5 s, on the first attempt and on the retry (16–23 s each). Locally, `--repeat-each 5 --retries 0` passed every time in about 4 s. The same test failed the same way on main at 402e55af (run 41080130065). It passed on main's later runs, and on this PR's next head.
**Why.** Not established. The toast is checked with the default 5 s `expect` timeout after the library reappears, and each CI run took 4–5 times longer than local. The toast may come and go before the check sees it, or come later than 5 s, on a slow runner.
**Cost.** About 10 minutes of reading logs and reproducing locally. The CI run that followed was needed anyway for another fix.
**Prevent by.** A bead to make this test robust on slow runners. It could check that the toast was shown at all (for example via a toast log the page keeps), rather than that it is visible within 5 s. Start by reading the trace in that run's Playwright artifact.
**Seen before.** none found in `docs/retrospectives/`. It is on main's CI history at 402e55af.
