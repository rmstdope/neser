# Retrospective — neser

Structured retrospective entries for AI-assisted workflows on the neser project.
Each entry captures what went well, what to improve, and which skills were used.

---

## 2026-05-24 - #2631 / PR #2638: Capture and restore GBA PPU state

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2638
**Linked issues:** #2625, #2631

### Customizations used

| Type | Name | Purpose |
| --- | --- | --- |
| Skill | `test-driven-development` | Guided RED -> GREEN -> REFACTOR -> COMMIT workflow for the PPU save-state slice. |
| Skill | `rust-developer` | Guided Rust implementation, serde state design, and validation. |
| Skill | `self-learning-skills` | Captured this retrospective after merge. |
| Agent | `code-review` | Reviewed changed PPU/save-state files before commit. |

### What went well

- The failing PPU tests covered the important save-state contract directly: register/affine restore, framebuffer/timing/frame-ready restore, and JSON round-trip.
- The implementation stayed consistent with earlier CPU and bus save-state slices by using an explicit `PpuState` plus `capture_state` / `restore_state` helpers.
- Local checks and PR CI both completed cleanly before merge, and issue/parent traceability was updated immediately after merge.

### What to improve

- The resumed workflow had already entered GREEN after compaction; future resumed TDD work should restate the current phase and approval/autonomy basis before the next edit.
- The session SQL todo update failed after merge due a local session database access issue. For future long-running autopilot sessions, treat SQL tracking as helpful but non-blocking and keep `plan.md`/GitHub issues as the durable source of truth.

### Navigator feedback

Navigator unavailable; feedback pending.

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

---

## 2026-05-18 - PR #2428: Fix GB LCDC window-map timing

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2428
**Linked issues:** #2355

### Customizations used

| Type | Name | Purpose |
| --- | --- | --- |
| Skill | `github-administration` | Supported issue, branch, and PR workflow handling. |
| Skill | `github-issue-designer` | Supported issue-focused framing and traceability. |
| Skill | `test-driven-development` | Guided enabling failing CRC tests before implementation and validating the fix. |
| Skill | `rust-developer` | Supported Rust implementation and local validation. |
| Skill | `gb-hardware-research` | Guided Game Boy LCDC/window-map timing investigation. |
| Skill | `rust-code-refactoring` | Supported focused cleanup around the PPU FIFO change. |
| Skill | `clean-coder` | Encouraged a narrow, maintainable implementation. |
| Agent | `code-review` | Provided review-oriented feedback before finalizing the PR. |
| Agent | `Iteration Retrospective Gatherer` | Produced the retrospective content after PR creation. |
| Instructions | Repository workflow instructions | Applied repository workflow expectations including TDD, validation, and PR discipline. |

### What went well

- The workflow paired `gb-hardware-research` with `test-driven-development`, using the five mealybug LCDC window-map CRC tests as concrete regression targets before finalizing the timing fix.
- The implementation stayed localized to `src/gb/ppu/pixel_fifo.rs`, which indicates the Rust/refactoring/clean-code guidance helped avoid a broad PPU rewrite.
- Focused unit coverage was added alongside the CRC tests, giving both targeted behavior checks and higher-level hardware-test confidence.
- The `code-review` agent was used as an explicit second-pass AI customization before the PR was considered complete.

### What to improve

- Several customizations used for the work package were not discoverable in the workspace paths inspected by the retrospective agent, so the retrospective had to rely on the user-provided list. Capture active customization names during the work package so the final retrospective can verify them without reconstruction.
- For timing-sensitive hardware fixes, explicitly record the key sampling-point rationale when the fix is made. That would make later retrospectives more precise about which research insight drove the implementation.
- When many skills are active, summarize each skill's concrete contribution before PR creation. This would make it easier to distinguish which customization materially affected the outcome versus which was only nominally selected.

### Navigator feedback

No additional feedback.

---

## 2026-05-20 — PR #2574: Fix GBA SRAM 32-bit SWI lane handling

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2574
**Linked issues:** none recorded

### Customizations used

| Type         | Name                      | Purpose                                                                                |
| ------------ | ------------------------- | -------------------------------------------------------------------------------------- |
| Skill        | `gba-hardware-research`   | Grounded the fix in the GBA memory-map spec: SRAM at 0x0E/0x0F is an 8-bit-only bus.  |
| Skill        | `gba-cpu-development`     | Guided ARM7TDMI SWI dispatch and BIOS assembly conventions for CpuSet/CpuFastSet.      |
| Skill        | `test-driven-development` | Structured the workflow around RED→GREEN→REFACTOR phase gates.                         |
| Skill        | `rust-developer`          | Guided Rust test implementation in `src/gba/bios/mod.rs`.                              |
| Skill        | `bug-hunter`              | Applied test-first bug-fix discipline before touching production code.                 |
| Skill        | `github-administration`   | Supported branch, PR creation, and merge workflow via `gh`.                            |
| Skill        | `self-learning-skills`    | Captured this retrospective after the PR was merged.                                   |
| Instructions | `copilot-instructions.md` | Applied repository workflow rules (TDD, pre-merge checks, retrospective requirement).  |

### What went well

- ✅ The fix was narrowly scoped: a single `cmp r4, #0xE` / `biclo` conditional added to both `swi_cpu_set` and `swi_cpu_fast_set` in `bios.s`. No unrelated code was touched.
- ✅ Test coverage was systematic and complete: both SRAM mirrors (0x0E and 0x0F), all three unaligned byte offsets (1, 2, 3), source and destination variants, and normal-memory regression tests. The test matrix was derived directly from the spec rather than observed behavior.
- ✅ The GBA bus layer already correctly mirrored SRAM bytes across a 32-bit word; identifying that only the BIOS SWI alignment logic was broken kept the fix scope minimal.
- ✅ The `gba-hardware-research` skill was the right primary skill: the SRAM 8-bit-bus property is a hardware-map fact, and anchoring the fix there prevented over-engineering.

### What to improve

- ❌ No linked issue number is traceable from the PR title or description. Each BIOS SWI fix should have a corresponding GitHub issue opened before branching, following the repository workflow rule.
- ❌ The alignment guard uses `biclo` ("clear if lower than 0xE"), which intentionally covers both 0x0E and 0x0F SRAM mirrors, but this is not commented in the assembly. A brief inline comment explaining the deliberate range would make the spec intent self-evident for future maintainers.
- ❌ The `gba-cpu-development` skill does not explicitly address the Rust-vs-assembly tradeoff for BIOS SWI shims. When a fix lives in `.s` source requiring a cross-assembler toolchain, the skill should prompt an explicit decision: is the assembly approach the right long-term choice, or should a Rust-level BIOS shim be preferred for testability?

### Navigator feedback

#### What went well

No feedback provided.

#### What to improve

No feedback provided.

---

## 2026-05-20 — PR #2581: Fix mealybug m3_obp0_change CGB-C rendering

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2581
**Linked issues:** #2580

### Customizations used

| Type | Name | Purpose |
| --- | --- | --- |
| Skill | `gb-hardware-research` | Guided model-specific GB/CGB PPU behavior investigation. |
| Skill | `bug-hunter` | Supported focused bug diagnosis and regression-oriented fix validation. |
| Skill | `test-driven-development` | Guided RED/GREEN/REFACTOR workflow and reference CRC activation. |
| Skill | `rust-developer` | Supported Rust implementation and required check validation. |
| Agent | `code-review` | Provided fallback changed-file review before PR handoff. |
| Agent | `Iteration Retrospective Gatherer` | Produced the retrospective content after PR creation; manual append was required. |
| Instructions | Repository workflow instructions | Applied issue assignment, branch, TDD, validation, and PR workflow expectations. |

### What went well

- The selected skills matched the problem well: `gb-hardware-research` was appropriate for CGB-C DMG-compat PPU palette behavior, while `bug-hunter` and `test-driven-development` anchored the fix in reproducible coverage.
- Activating the Mealybug `m3_obp0_change` CGB-C reference CRC `0x7484_BAF1` created a hardware-test regression guard for the exact edge case being fixed.
- Focused OBP0/OBP1 unit tests complemented the Mealybug acceptance test, giving both narrow behavior coverage and high-level compatibility validation.
- The `code-review` fallback agent found one useful test isolation improvement before the PR handoff.

### What to improve

- The retrospective agent could generate the entry but could not write the file directly. Keep the manual append step in mind when the agent reports that limitation.
- For future GB PPU timing/palette fixes, capture the exact edge-behavior rationale in the code or PR when the fix is made, especially when model-specific CGB-C behavior changes at an OBJ fetch boundary.
- The workflow involved several active skills. Summarizing each skill's concrete contribution before PR creation made the retrospective more reliable and should remain part of future handoffs.

### Navigator feedback

Pending — navigator unavailable during retrospective collection.

---

## 2026-05-24 — PR #2640: Fix mGBA Memory BIOS open-bus behavior

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2640
**Linked issues:** none

### Customizations used

| Type | Name | Purpose |
| --- | --- | --- |
| Skill | `test-driven-development` | Structured the work around TDD phase gates until navigator unavailable. |
| Skill | `gba-hardware-research` | Grounded the BIOS open-bus behavior investigation in GBA hardware behavior. |
| Skill | `bug-hunter` | Supported focused diagnosis and regression-oriented validation. |
| Skill | `rust-developer` | Guided Rust implementation and validation work. |
| Skill | `rust-code-refactoring` | Supported cleanup and refactor review after the functional fix. |
| Agent | `code-review` | Caught the expensive embedded BIOS slice comparison and unclear vector special case during refactor. |
| Agent | `Iteration Retrospective Gatherer` | Produced this retrospective content after PR creation; manual append was required. |

### What went well

- The TDD phase gates were followed through the navigator-guided portion of the work, preserving a disciplined RED/GREEN/REFACTOR structure before autonomous approval mode became necessary.
- The `code-review` agent added concrete value during refactor by identifying an expensive embedded BIOS slice comparison and an unclear vector special case, producing actionable refinement targets rather than generic feedback.
- Local pre-merge check failure from disk exhaustion was recovered without abandoning validation: `cargo clean`, `CARGO_INCREMENTAL=0`, and running Rust-heavy checks individually allowed checks to complete under constrained local resources.

### What to improve

- When navigator availability ends mid-workflow, explicitly record the transition into autonomous approval mode and the current TDD phase before continuing, so the retrospective does not have to reconstruct where the phase gate changed.
- Pre-merge validation should account for local disk pressure earlier on Rust-heavy workflows; when incremental artifacts are likely to be large, start with `CARGO_INCREMENTAL=0` or staged check execution instead of discovering the issue during final checks.
- PR creation first failed because the stacked base branch had been deleted. Before opening future PRs, verify the intended base branch still exists and rebase onto `origin/main` proactively when the stacked dependency has already landed or disappeared.

### Navigator feedback

No additional feedback — navigator unavailable during retrospective collection.

---

## 2026-05-24 — PR #2623: Fix daid CGB speed-switch LY/STAT timing

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2623
**Linked issues:** #2612

### Customizations used

| Type | Name | Purpose |
| --- | --- | --- |
| Skill | `gb-hardware-research` | Supported Game Boy Color STOP/KEY1 timing investigation for LY/STAT speed-switch behavior. |
| Skill | `test-driven-development` | Structured the implementation around RED, GREEN, REFACTOR, COMMIT, and MERGE gates. |
| Skill | `rust-developer` | Guided Rust implementation and test coverage for the CGB bus timing changes. |
| Skill | `github-administration` | Supported issue assignment, PR review-thread handling, CI checks, and merge workflow through `gh`. |
| Skill | `self-learning-skills` | Triggered the required post-merge retrospective and skill update. |
| Agent | `code-review` | Reviewed the changed timing code before PR creation. |
| Agent | `Iteration Retrospective Gatherer` | Produced retrospective content after the PR merged; manual append was required. |
| Instructions | Repository workflow instructions | Applied branch, TDD, validation, review, CI, merge, and retrospective requirements. |

### What went well

- The work waited for the overlapping #2611 PR to merge before branching, avoiding churn in `daid_tests.rs`, `ppu.rs`, and `timing.rs`.
- The RED phase activated the daid LY/STAT CRCs against upstream reviewed baselines and added focused speed-switch timing coverage before implementation.
- Review comments were fetched with `gh`/GraphQL, addressed with code changes, replied to, and resolved before merge.
- Full local checks and GitHub CI were both green before merging PR #2623.

### What to improve

- The review-thread GraphQL query shape was useful but not fully documented in the GitHub administration skill before this PR. Add the fetch/reply/resolve pattern so future review-response loops are faster.
- The retrospective agent again could generate the entry but could not write the file directly. Keep the manual append step in mind when the agent reports that limitation.
- Navigator feedback was unavailable during retrospective collection. Keep feedback pending/unavailable rather than blocking the retrospective.

### Navigator feedback

Pending/unavailable — navigator was unavailable during retrospective feedback collection.

---

## 2026-05-24 — PR #2627: Fix MBC3 RTC sub-second timing

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2627
**Linked issues:** #2619

### Customizations used

| Type | Name | Purpose |
| --- | --- | --- |
| Skill | `bug-hunter` | Guided issue verification, reproduction, and focused regression-oriented bug fixing. |
| Skill | `test-driven-development` | Structured the work into RED/GREEN/REFACTOR iterations for MBC3 timing, CGB RTC scaling, and acceptance CRC approval. |
| Skill | `gb-hardware-research` | Grounded MBC3 RTC behavior in Pan Docs first, then SameBoy implementation evidence where Pan Docs was incomplete. |
| Agent | `code-review` | Reviewed MBC3, CGB/save-state, and final changed-file behavior before PR creation. |
| Agent | `Iteration Retrospective Gatherer` | Produced retrospective content after PR creation; manual append was required. |
| Instructions | Repository workflow instructions | Applied issue assignment, branch, TDD, validation, visual approval, PR creation, and review-first merge workflow. |

### What went well

- The initial planning interview resolved key design branches before code changes: SameBoy-backed seconds-write reset, halt/resume preservation, CGB double-speed RTC scaling, save-state version bump, and visual approval before CRC updates.
- The TDD slices kept the hardware-timing work reviewable: focused MBC3 unit tests, focused CGB bus/save-state tests, then `rtc3test-3` acceptance activation.
- `gb-hardware-research` was useful because Pan Docs did not define fractional RTC write timing; the workflow clearly labeled SameBoy as implementation evidence rather than primary specification.
- The first full validation exposed a local disk-space issue during Wasm tests. Cleaning generated wasm target artifacts and rerunning the full suite after rebasing gave a clean merge-ready validation.

### What to improve

- The retrospective agent again could generate the entry but could not write the file directly. Keep the manual append step in mind when the agent reports that limitation.
- For MBC/RTC timing issues, capture the key research conclusion in the PR body: which behavior is spec-confirmed, which behavior is implementation-evidence-backed, and which acceptance ROM proves the result.
- When a plan expands from the original issue, as with CGB double-speed RTC scaling here, explicitly call out why the added scope belongs in the same PR.

### Navigator feedback

No additional feedback.

---

## 2026-05-24 — PR #2635: Enable GBA save-state disk routing

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2635
**Linked issues:** #2628

### Customizations used

| Type | Name | Purpose |
| --- | --- | --- |
| Skill | `github-issue-designer` | Supported sub-issue framing and scope traceability for #2628. |
| Skill | `github-administration` | Supported issue metadata, branch, and PR workflow for #2635. |
| Skill | `test-driven-development` | Guided RED/GREEN/REFACTOR/COMMIT flow and save/load state toast tests. |
| Skill | `rust-developer` | Guided Rust implementation across GBA state path and console routing code. |
| Skill | `gba-cpu-development` | Provided GBA-specific emulator context while planning save-state routing behavior. |
| Skill | `self-learning-skills` | Triggered issue-creation and PR retrospectives. |
| Agent | `code-review` | Reviewed changed files for routing/path/toast issues before PR creation. |
| Agent | `Iteration Retrospective Gatherer` | Produced this retrospective content after PR creation; manual append was required. |
| Instructions | Repository workflow instructions | Applied small-increment TDD, GitHub issue workflow, validation, and review-first PR requirements. |

### What went well

- The selected skills matched the work package: `rust-developer` and `gba-cpu-development` fit the GBA save-state implementation, while `test-driven-development` anchored the routing fix in failing save/load toast coverage.
- The implementation stayed narrow: storing the loaded GBA ROM path, adding `Gba::state_path()`, and routing `Console::GameBoyAdvance` through it directly addressed the disk-routing gap without broad save-state redesign.
- The tests cover user-visible behavior through the shared disk save/load path, not just low-level path construction.
- The workflow discovered a local disk-space blocker during RED verification and resolved it by cleaning repository build artifacts before continuing.

### What to improve

- Navigator feedback could not be collected during this retrospective. Keep it pending rather than inventing feedback.
- The retrospective agent could generate the entry but could not write the file directly. Continue using the manual append fallback when that limitation appears.
- The `gh issue-child-add` extension was unavailable, so #2625 child tracking used explicit issue references. If hierarchical linking is important for future work, install or document the extension setup before large split-issue workflows.

### Navigator feedback

Pending — navigator unavailable during retrospective collection.
