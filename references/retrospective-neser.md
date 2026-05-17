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

---

## 2026-05-17 - PR #2414: Improve GB LCDC TILE_SEL timing

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2414
**Linked issues:** #2353, #2412

### Customizations used

| Type | Name | Purpose |
| --- | --- | --- |
| Skill | `test-driven-development` | Kept each timing slice anchored to focused tests and RED/GREEN/REFACTOR gates. |
| Skill | `gb-hardware-research` | Guided Game Boy PPU/LCDC timing investigation against Pan Docs and hardware-oriented behavior. |
| Skill | `rust-developer` | Guided Rust implementation and validation work. |
| Skill | `rust-code-refactoring` | Supported small cleanup steps after functional fixes. |
| Skill | `github-issue-designer` | Structured follow-up issue #2412 with explicit remaining scope and acceptance criteria. |
| Skill | `github-administration` | Supported issue creation, PR creation, branch push, and CI verification through `gh`. |
| Skill | `self-learning-skills` | Captured required issue-creation feedback; navigator had no additional feedback. |
| Agent | `code-review` | Reviewed the branch before PR creation; a post-rebase review attempt timed out, so local checks and CI were used as final validation. |
| Agent | `Iteration Retrospective Gatherer` | Produced this retrospective content after PR creation. |
| Instructions | Repository workflow instructions | Enforced small increments, issue linkage, review-first PR workflow, and required pre-PR checks. |

### What went well

- The TDD slices kept a difficult hardware-timing problem manageable: each OBJ/window TILE_SEL case added a focused pixel FIFO test before the production timing adjustment.
- The follow-up issue split was useful: #2353 delivered the DMG non-window CRC and several DMG window slices, while #2412 explicitly preserves the remaining DMG OAM X=8 and CGB/CGB-D work.
- The GitHub administration workflow caught a stale base before PR creation; rebasing onto `origin/main` before opening the PR avoided carrying avoidable merge churn into review.
- Full local checks plus PR CI provided a strong handoff after the branch was rebased and force-pushed.

### What to improve

- The TILE_SEL work had many narrow timing commits. For similar PPU timing issues, create an explicit timing matrix early: BG/window, low/high byte, visible/off-left OBJ, delayed/stalled fetch, and model variant.
- A broad OAM X=4 OBJ penalty change regressed accepted mealybug and Mooneye timing before it was narrowed back. Future timing work should isolate global scheduler changes from local LCDC sampling fixes unless a test proves the scheduler itself is wrong.
- The retrospective agent could generate the entry but could not write the file directly. Keep the manual append step in mind when the agent reports that limitation.

### Navigator feedback

No additional feedback.
