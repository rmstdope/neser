# nr-phv: retrospective

- **Implementer:** Bishop
- **Date:** 2026-09-27
- **PR:** #3224

## A lead that first shows at frame 506 started at power-on, from a reset the frontend adds

**What happened.** The bead placed the one-frame lead at the first animated frame of
Street Fighter Alpha 2's Capcom logo, around frame 506. Per-frame WRAM dumps against Mesen2
showed NESER a frame ahead by frame 7. Every screen before 506 was static, so nothing
visible showed it. The cause was not in the emulation. Desktop and headless call
`reset(false)` right after `load_rom`, which has already reset the CPU. The second reset ran
the 186-clock startup delay again, so the first instruction ran at master clock 372 instead
of Mesen2's 186. The web frontend never resets after loading, so it was right all along.
From then on, a race between an NMI and an RDNMI poll resolved the other way.
**Why.** Nothing compared NESER's clock at the first instruction with Mesen2's. The existing
unit tests call `load_rom` directly, so they never see the `reset(false)` the frontends add.
The screenshot-based validation cannot see a lead while the screen is static.
**Cost.** About two hours on frame-end state dumps and instruction traces aligned from frame
456 back to frame 3, before a power-on trace showed the 186-clock offset in its first line.
**Prevent by.** In `snes-hardware-research`, the first step of any "NESER runs N frames
ahead or behind Mesen2" investigation: run a Mesen2 exec trace with `masterClock` and a
NESER trace through the same `--headless` path, and compare the clock of the first
instruction and the first hundred before anything else. The test
`hard_reset_straight_after_load_keeps_the_power_on_clock` now pins load plus hard reset at
186.
**Seen before.** None found (`grep -rli "startup delay\|power-on clock"
docs/retrospectives/` is empty).

## The first full gate failed `cargo fmt --check`: the worktree has no `core.hooksPath`

**What happened.** The same as nr-6e9 and nr-hab.1: the prepared worktree does not set
`core.hooksPath`, so `.githooks/pre-commit` never formatted the staged Rust.
**Why.** As nr-6e9 says.
**Cost.** One wasted full-gate run, about fifteen minutes.
**Prevent by.** As nr-6e9 says: `scripts/prepare-worktree` runs `git config core.hooksPath
.githooks`.
**Seen before.** nr-6e9, nr-hab.1.
