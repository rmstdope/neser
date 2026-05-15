# Retrospective — neser

Structured retrospective entries for AI-assisted workflows on the neser project.
Each entry captures what went well, what to improve, and which skills were used.

---

## 2026-05-13 — #2347 Fix mealybug m2_win_en_toggle CGB-C and CGB-D (PR #2364)

### What went well
- Root-cause identification was accurate and evidence-based: pixel analysis (PIL/Python) surfaced the real bug (wrong CGB DMG-compat palette for non-Nintendo licensees) rather than following the misleading PPU timing framing in the issue title.
- TDD cycle was tight and clean: RED (failing test asserting reference CRC computed from real hardware PNG) → GREEN (remove 3 lines) → REFACTOR (rubber-duck review found nothing to change). Fix was proportional to the defect.
- Using SameBoy `cgb_boot.asm` and Pan Docs as primary spec references grounded the fix at algorithm level, not cargo-culted observed behavior.
- The `gb-hardware-research` skill's "prefer written spec over emulator implementation" principle steered verification correctly.

### What to improve
- **Branching from sibling PR instead of main**: Branching from #2363 instead of `main` introduced rebase overhead after #2363 merged. The rule is to always branch from the latest main; if a dependency on an open PR is unavoidable, that constraint must be flagged explicitly and agreed with the navigator before branching.
- **Dead-path utilities after refactor**: `is_nintendo_licensee()` remains in the module but is no longer called by `get_palette_id()`. The REFACTOR phase should flag dead-path utilities for cleanup or re-scoping rather than leaving them as silent noise.
- **Issue title vs. evidence mismatch**: The issue title implied a PPU/timing bug; the actual cause was a palette initialization bug. Surface title-vs-evidence mismatches early (during planning) so investigation doesn't follow a misleading framing.

### Skills used
- `test-driven-development`
- `gb-hardware-research` (for spec verification)
- `self-learning-skills` (this retrospective)

---

## 2026-05-13 — PR #2372: Fix mealybug BGP FIFO

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2372
**Linked issues:** #2348

### Customizations used

| Type | Name | Purpose |
| --- | --- | --- |
| Skill | `gb-hardware-research` | Grounded Game Boy rendering behavior against hardware-oriented evidence. |
| Skill | `test-driven-development` | Structured the workflow around RED->GREEN->REFACTOR phase gates. |
| Skill | `rust-developer` | Guided Rust implementation and validation work. |
| Skill | `rust-code-refactoring` | Supported focused cleanup after functional changes. |
| Skill | `github-administration` | Supported issue/PR workflow administration. |
| Skill | `self-learning-skills` | Captured this retrospective and learning outcome. |
| Agent | `code-review` | Provided review-oriented feedback on the implementation. |
| Instructions | `copilot-instructions.md` | Applied repository workflow rules, including TDD and retrospective expectations. |

### What went well

- Multiple relevant skills were combined effectively: hardware research for Game Boy behavior, TDD for workflow structure, Rust skills for implementation, and code review for validation.
- After the TDD phase gate was re-established, the workflow followed RED->GREEN->REFACTOR discipline for the remaining work.
- The retrospective correctly identified that the existing TDD skill instructions already cover the resumed/compacted workflow issue, so no unnecessary skill documentation update was made.

### What to improve

- A resumed/compacted workflow briefly advanced a TDD RED->GREEN step before explicitly re-establishing the TDD phase gate; on resume, restate the current phase and next allowed action before making implementation progress.
- The workflow should treat compaction/resume boundaries as process checkpoints: reload or restate active skills and gates before continuing code changes.

### Navigator feedback

No feedback provided.
