# nr-d9x — retrospective

## A mapper fix shipped for a board whose one game never booted

**What happened.** The first pass (PR #3286) fixed the bead as filed: NES-EVENT PRG-RAM was gated by the timer bit. Every unit test passed, the review found nothing blocking, and it merged. At verification the navigator ran *Nintendo World Championships 1990*, the only game on the board, and got a solid green screen. The rework reproduced it headlessly in one command: mapper 105 banked PRG as plain MMC1, so at power-on bank 15 of the second chip sat at $C000 and the CPU started from the wrong reset vector. Its IRQ timer was also a 1-16 cycle countdown instead of nesdev's 30-bit counter.

**Why.** The bead was scoped to one defect, which a review had found in the code, and the plan said "No human check: the change is emulated RAM behaviour only." Nobody ran the game. The unit tests pinned the existing MMC1-style banking as correct (`mapper_105_prg_bank_switching_matches_mmc1_mode_3_at_8000_bfff`), so the suite agreed with the bug.

**Cost.** One failed verification, a reopened bead and a second full pass, including a full gate, a review and CI.

**Prevent by.** In `produce-bead`, *The plan* → *Validation*: when a bead changes a mapper and a known game on that mapper is available locally, add a headless boot check. That check is `neser --headless --frames 600 --output …`, compared against Mesen2 with the `scripts/reference_capture/README.md` recipe. "Nothing a person sees changes" does not excuse it, because the game running is the only thing a player sees. Here that check turns the green screen up before the first merge, in seconds.

**Seen before.** Not in `docs/retrospectives/`.
