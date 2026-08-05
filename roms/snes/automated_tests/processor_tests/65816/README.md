# 65816 ProcessorTests (pinned)

This directory contains a pinned subset of Tom Harte's ProcessorTests `65816/v1` vectors for git tracking.

Source repository: `https://github.com/SingleStepTests/ProcessorTests`

Pinned upstream commit: `bb11756436da8fd16cce86aef63dc6725f48836f`

Tracked subset files: `v1/*.json` — one emulation/native pair per selected opcode, truncated to 32 vectors per file, regenerated deterministically by `scripts/refresh_65816_processor_tests_subset.py`. The script's coverage report is the authoritative selection listing; this README intentionally does not duplicate the file list.

Optional full corpus location (local, ignored by git):

- `full/v1/*.json`

SNES integration tests in `src/snes/integration_tests/processor_tests_65816.rs` use the tracked subset by default and automatically switch to full-corpus files (per filename) when `full/v1` exists.

To refresh the local full corpus from upstream, run:

`bash scripts/refresh_65816_processor_tests_subset.sh`

## Known-divergent vectors (#3135)

29 vectors in the full corpus carry expectations that are known-wrong for the SNES 5A22, and the test harness skips them by name (`KNOWN_DIVERGENT_VECTORS` in `processor_tests_65816.rs`, whose doc comment holds the full evidence trail):

- 28 `a1 e` vectors — LDA (dp,X) with E=1, DL != 0 and the pointer straddling a page boundary: the vectors carry the pointer high-byte fetch into the next page, but real hardware wraps it within the page (gilyon cputest tests 02c9–02cc, validated on real hardware; Mesen2 agrees).
- `d4 e 232` — PEI with E=1 and DL == 0: the vector wraps the pointer fetch within the direct page, but PEI never wraps (WDC datasheet, Bruce Clark's 65C816 tutorial section 5.11, gilyon cputest test 03c4, Mesen2).

The corpus is generated from an emulator model rather than captured from hardware, and has a history of exactly this class of emulation-mode wrap bug: upstream issue #1 led to the long-indirect `[dp]`/`[dp],Y` vectors being regenerated, and issues #3, #6 and #8 were still open at the pinned commit. None of the divergent vectors appear in the tracked CI subset.
