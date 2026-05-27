# Retrospective — neser

Structured retrospective entries for AI-assisted workflows on the neser project.
Each entry captures what went well, what to improve, and which skills were used.

---

## 2026-05-24 - #2632 / PR #2639: Capture and restore GBA APU state

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2639
**Linked issues:** #2625, #2632

### Customizations used

| Type | Name | Purpose |
| --- | --- | --- |
| Skill | `test-driven-development` | Guided RED -> GREEN -> REFACTOR -> COMMIT workflow for the APU save-state slice. |
| Skill | `rust-developer` | Guided Rust serde derives, snapshot design, and validation. |
| Skill | `self-learning-skills` | Captured this retrospective after merge. |
| Agent | `code-review` | Reviewed changed APU/save-state files before commit. |

### What went well

- The tests covered both observable register/FIFO/wave restore and private timing accumulators, including preserving the frontend's current runtime output sample rate.
- The implementation reused the established explicit state-object pattern from CPU and PPU save-state work.
- A local wasm disk-space/incremental-build issue was handled safely by rerunning wasm with `CARGO_INCREMENTAL=0`, matching the same behavior without deleting unrelated artifacts.

### What to improve

- The RED test initially expected a readable CH1 length-enable bit after a trigger that did not set length enable; future APU tests should double-check write-only/readable register masks before locking assertions.
- A concurrent PR opened against overlapping GBA save-state files while starting the next sub-issue. Future sub-issue starts should treat open-overlap PRs as a hard branching checkpoint before creating the next branch.

### Navigator feedback

Navigator unavailable; feedback pending.

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

### Post-merge update

**Merge commit:** f3925a384f61de827f56b95b38080605e5c9e920

#### Additional customizations used

| Type | Name | Purpose |
| --- | --- | --- |
| Skill | `github-administration` | Supported PR review-thread replies, resolution, CI checks, merge verification, and branch cleanup. |
| Skill | `self-learning-skills` | Helped capture post-merge iteration learning for future improvement. |

#### What went well

- Review follow-up was handled systematically: three Copilot review threads were addressed, replied to, resolved, and verified before merge.
- The implementation was strengthened with `dma_latch_valid` plus a zero-DMA-latch regression test, turning review feedback into executable coverage.
- Stale `restore_memory_state` documentation was updated as part of the review response, reducing future confusion around behavior.
- The PR was rebased onto `main`, local checks passed, CI was verified green, and the branch was cleaned up after merge.

#### What to improve

- Navigator feedback was unavailable at retrospective time; when possible, capture navigator feedback before merge or immediately after review resolution.
- Hardware-behavior fixes relied on multiple specialized skills across the iteration; consider consolidating the useful GBA open-bus/DMA latch findings into an existing skill or reference so future fixes start with less rediscovery.
- Documentation drift was only caught during review follow-up; add an explicit docs-consistency check when changing emulator state-restore behavior.

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
---

## 2026-05-25 — PR #2657: Automate GB acid which model checks

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2657
**Linked issues:** #2651

### Customizations used

| Type | Name | Purpose |
| --- | --- | --- |
| Skill | `test-driven-development` | Guided the acid/which.gb automation through regression-focused coverage and validation. |
| Skill | `rust-developer` | Supported Rust implementation quality, formatting, clippy compliance, and save-state updates. |
| Skill | `gb-hardware-research` | Helped interpret model-specific GB/CGB behavior for extra-OAM and $FEA0-$FEFF reads. |
| Skill | `github-administration` | Supported PR and linked-issue workflow context. |
| Agent | `code-review agent fallback` | Provided fallback review-oriented scrutiny when dedicated review flow was needed. |
| Agent | `Iteration Retrospective Gatherer` | Captured AI-assisted workflow learnings for this retrospective. |

### What went well

- The `gb-hardware-research` customization was well matched to the core issue: identifying model-specific behavior needed for which.gb to distinguish CGB-D and CGB-E.
- The `test-driven-development` skill aligned the work around durable automated coverage by adding non-ignored CRC checks across DMG and CGB model variants instead of relying on manual ROM inspection.
- The `rust-developer` skill supported a clean Rust implementation path, including persistence of CGB extra-OAM RAM in save states and successful completion of clippy, fmt, native, wasm, script, and npm validation.
- The retrospective gatherer had sufficient PR context and validation details available, reducing the need for additional reconstruction after the PR was created.

### What to improve

- The workflow depended on several customizations at once; future iterations should explicitly record which customization contributed to each major decision while the work is happening, especially for hardware-research findings.
- The code-review fallback was used as a fallback rather than a planned checkpoint; for hardware-sensitive emulator changes, schedule review-oriented scrutiny earlier before the full validation sweep.
- The acid ROM asset consolidation and model-behavior implementation were both part of the same package; future AI-assisted workflows should separate asset-layout decisions from hardware-behavior reasoning in notes to make later review easier.

### Navigator feedback

No additional feedback.

---

## 2026-05-25 - PR #2658: Automate remaining Blargg GB timing rows

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2658
**Linked issues:** #2652

### Customizations used

| Type | Name | Purpose |
| --- | --- | --- |
| Skill | `github-administration` | Guided PR creation, merge flow, and issue/PR tracking. |
| Skill | `test-driven-development` | Enforced strict RED/GREEN/REFACTOR/COMMIT/MERGE gates. |
| Skill | `rust-developer` | Guided Rust integration-test and helper changes. |
| Skill | `self-learning-skills` | Guided post-merge retrospective capture. |
| Agent | `code-review` | Reviewed changed Blargg test code during REFACTOR. |
| Agent | `Iteration Retrospective Gatherer` | Produced retrospective content after merge; manual append was required. |

### What went well

- The strict RED/GREEN/REFACTOR/COMMIT/MERGE gates kept the workflow controlled and reviewable.
- User feedback before GREEN was incorporated cleanly by removing `shootout` from test names before finalizing the test suite.
- The GREEN implementation stayed focused on `src/gb/integration_tests/blargg_tests.rs`, adding Blargg timing coverage and making the LCD helper generic over `GbBus`.
- The full checkpoint suite passed before PR creation and merge.

### What to improve

- Naming concerns surfaced after the initial RED proposal; future RED phases should include an explicit quick naming review before asking for GREEN approval.
- The need to make the LCD helper generic over `GbBus` emerged during GREEN; future similar test automation should inspect helper type constraints earlier during RED planning.
- The retrospective agent could generate the entry but could not write the file directly. Continue using the manual append fallback when that limitation appears.

### Navigator feedback

No feedback provided.

---

## 2026-05-25 — PR #2661: Fix #2655: automate Ashiepaws GB shootout tests

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2661
**Linked issues:** #2655

### Customizations used

| Type         | Name                        | Purpose                                                                 |
| ------------ | --------------------------- | ----------------------------------------------------------------------- |
| Skill        | `github-issue-designer`     | Supported issue-scoped framing and traceability for the Ashiepaws rows. |
| Skill        | `github-administration`     | Guided PR, issue, and full pre-merge checklist workflow.                |
| Skill        | `test-driven-development`   | Enforced RED/GREEN/REFACTOR/COMMIT approval gates for the automation.   |
| Skill        | `rust-developer`            | Guided Rust test automation and validation work.                        |
| Skill        | `gb-hardware-research`      | Grounded OAM DMA behavior decisions in Game Boy hardware behavior.      |
| Skill        | `bug-hunter`                | Kept the workflow focused on reproducible regression coverage.          |
| Skill        | `self-learning-skills`      | Triggered retrospective capture after the PR was created.               |
| Agent        | `code-review`               | Provided fallback review-oriented scrutiny during the refactor phase.   |
| Instructions | `copilot-instructions.md`   | Applied repository workflow rules for TDD, review, and validation.      |

### What went well

- ✅ The customization mix matched the work: `test-driven-development` controlled the RED/GREEN/REFACTOR/COMMIT gates while `gb-hardware-research` supplied the OAM DMA context needed for hardware-sensitive Ashiepaws shootout automation.
- ✅ The issue and GitHub workflow customizations kept PR #2661 tied back to #2655 and carried the work through the full pre-merge checklist instead of stopping at local test automation.
- ✅ The code-review fallback was used at the right risk point during REFACTOR, adding a second AI-assisted review pass without bypassing the TDD approval gates.
- ✅ The workflow converted Ashiepaws GBEmulatorShootout rows into automated coverage, making future regressions detectable by the test suite rather than manual shootout inspection.

### What to improve

- ❌ Many skills were active at once; future iterations should record which customization drove each major decision, especially where hardware research changes the expected OAM DMA behavior.
- ❌ The code-review agent was used as a fallback during refactor; for hardware-sensitive test automation, schedule review-oriented scrutiny as an explicit planned checkpoint before the final validation sweep.
- ❌ The OAM DMA research outcome should be summarized near the tests or PR rationale so future Ashiepaws row automation does not have to reconstruct why the expected behavior is hardware-accurate.

### Navigator feedback

#### What went well

No feedback provided

#### What to improve

No feedback provided

---

## 2026-05-26 — PR #2679: Fix GBA mGBA SIO timing diagnostics to 4/4

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2679
**Linked issues:** none

### Customizations used

| Type | Name | Purpose |
| --- | --- | --- |
| Skill | `test-driven-development` | Guided the RED/GREEN/REFACTOR/COMMIT/MERGE workflow and phase gates. |
| Skill | `bug-hunter` | Supported reproducing and reducing the SIO timing failure before implementation. |
| Skill | `rust-developer` | Guided Rust implementation, save-state compatibility, and validation. |
| Skill | `gba-hardware-research` | Grounded SIOCNT mode/start/timing behavior against GBA hardware references and mGBA suite expectations. |
| Skill | `github-administration` | Supported PR creation, review-thread replies/resolution, CI checks, rebase, and merge. |
| Skill | `self-learning-skills` | Captured post-PR and post-merge feedback and retrospective observations. |
| Agent | `code-review` | Reviewed the changed SIO-related files before commit; no significant issues found. |
| Agent | `Iteration Retrospective Gatherer` | Produced this retrospective content after merge; manual append was required. |

### What went well

- The TDD flow kept the fix anchored to focused acceptance diagnostics: SIO timing moved from `0/4` to `4/4`, while SIO read remained `90/90`.
- The implementation stayed localized to SIO state, bus-level start delay, CPU-visible SIOCNT write routing, save-state persistence, and mGBA diagnostic helpers.
- Review feedback was addressed before merge by clarifying mode-dependent SIOCNT bit 7 semantics and resolving both review threads.
- CI caught that `main` had advanced after PR #2678; rebasing onto latest `origin/main` and rerunning the full check set produced a green merge-ready branch.

### What to improve

- The first CI polling loop treated an empty `statusCheckRollup` immediately after force-push as complete. Future GitHub workflows should require at least one check entry before considering CI finished.
- The branch was not rebased immediately after review feedback before the first CI wait. Future review-fix workflows should fetch/rebase `origin/main` before pushing review updates when another PR has merged.
- The retrospective agent again produced content but could not write the file directly, so manual append remains necessary when that limitation appears.

### Navigator feedback

#### What went well

No feedback provided.

#### What to improve

No feedback provided.

---

## 2026-05-26 — PR #2666: Fix mGBA timer/timing regression

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2666
**Linked issues:** none

### Customizations used

| Type         | Name                               | Purpose                                                                                  |
| ------------ | ---------------------------------- | ---------------------------------------------------------------------------------------- |
| Skill        | `gba-hardware-research`            | Supported GBA timer/timing behavior reasoning, including delayed IRQ and count-up cases. |
| Skill        | `test-driven-development`          | Kept the regression fix anchored to mGBA suite coverage and updated approvals.           |
| Skill        | `rust-developer`                   | Supported Rust implementation, state persistence changes, formatting, and clippy-clean validation. |
| Agent        | `Iteration Retrospective Gatherer` | Captured AI-assisted workflow observations after PR creation.                            |
| Instructions | `copilot-instructions.md`          | Applied repository expectations for TDD, validation, and retrospective capture.          |

### What went well

- ✅ The GBA-focused customization fit the defect: the fix preserved side-effect-free timer register reads while keeping delayed timer IRQ/count-up behavior intact, avoiding a broad timing rewrite.
- ✅ The workflow kept generated and approval artifacts aligned with the source change by rebuilding the embedded BIOS and updating the affected mGBA suite approvals in the same iteration.
- ✅ Validation matched the repository instruction set across targeted GBA tests, full Rust/no-defaults checks, wasm, Python script tests, npm tests, clippy, and formatting.

### What to improve

- ❌ The retrospective had to reconstruct customization usage from the handoff summary and repository conventions. Future PR handoffs should explicitly list which skills/agents/prompts were active and what each contributed.
- ❌ Timer-global-cycle persistence was an important save/restore detail. Future timer/timing work should use an explicit checklist for state fields that affect delayed events, IRQ scheduling, and count-up propagation.
- ❌ BIOS rebuilds plus approval updates are easy to miss in emulator timing fixes. Record the exact rebuild/approval sequence in the work notes or PR body whenever generated BIOS artifacts or suite approvals change.

### Navigator feedback

#### What went well

No feedback provided

#### What to improve

No feedback provided

---

## 2026-05-27 - #2683 / PR #2692: Automate mGBA video subtests

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2692
**Linked issues:** #2683, #2687, #2688, #2689, #2690

### Customizations used

| Type | Name | Purpose |
| --- | --- | --- |
| Skill | `test-driven-development` | Guided RED -> GREEN -> REFACTOR -> COMMIT gates and the mini RED/GREEN loop for the START-overlay runner correction. |
| Skill | `github-issue-designer` | Structured the parent issue update and per-subtest tracking issues with scoped acceptance criteria. |
| Skill | `github-administration` | Guided safe `gh` issue/PR mutation with body files and immediate verification. |
| Skill | `gba-hardware-research` | Kept the work in the GBA test/hardware domain and preserved source-backed behavior assumptions. |
| Skill | `self-learning-skills` | Triggered retrospectives after issue creation/closure events. |
| Agent | `code-review` | Reviewed changed GBA runner/test/docs files for correctness and missed edge cases. |
| Agent | `Iteration Retrospective Gatherer` | Prepared the PR-creation retrospective entry. |

### What went well

- The plan-gathering step resolved the key design choice up front: passing video subtests should assert `actual == expected`, while non-passing subtests should remain individually ignored with tracking issues.
- Navigator feedback during GREEN caught a real runner issue: the mGBA suite's Actual/Expected overlay had to be hidden with START before CRC and screenshot capture.
- The START timing fix improved the result from 0/7 to 3/7 passing, letting Basic Mode 3, Basic Mode 4, and Degenerate OBJ transforms run in normal CI.
- Remaining failures are no longer hidden behind one ignored test; each has a dedicated ignored assertion and tracking issue.

### What to improve

- Tracking sub-issues were created before rechecking the runner's overlay-control assumptions, so three had to be closed immediately after the START fix. Future visual-test automation should validate suite UI controls before splitting follow-up issues.
- The retrospective gatherer could prepare an entry but could not write the file directly, so the main agent had to append it manually. Future retrospective prompts should include full PR metadata and be ready for manual fallback.

### Navigator feedback

No additional feedback.

---

## 2026-05-27 — PR #2694: Fix web handheld audio noise

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2694
**Linked issues:** none

### Customizations used

| Type         | Name                      | Purpose                                                                                |
| ------------ | ------------------------- | -------------------------------------------------------------------------------------- |
| Skill        | `bug-hunter`              | Guided diagnostic process to identify audio noise root cause across GB/GBA frontends.  |
| Skill        | `test-driven-development` | Structured development workflow with test-first approach for audio system changes.     |
| Skill        | `rust-developer`          | Guided Rust implementation of audio queue architecture and sample rate propagation.    |
| Instructions | `copilot-instructions.md` | Applied repository workflow rules including small increments and validation gates.     |

### What went well

- ✅ **Cross-platform audio architecture diagnosis**: The fix correctly identified that the underlying issue spanned both GB/CGB and GBA frontends, requiring a unified solution rather than platform-specific patches.
- ✅ **Bounded queue design**: Changing from single pending sample to a bounded queue architecture addresses the fundamental mismatch between frontend sample rate and APU output rate, preventing both underruns and unbounded buffering.
- ✅ **Save-state compatibility preservation**: The fix maintained compatibility with the immediately previous save-state version, showing awareness of user impact and migration cost.
- ✅ **Single-commit delivery**: The entire fix was delivered in one cohesive commit (`7acecac9`), indicating clear understanding of the problem scope and solution architecture before implementation.

### What to improve

- ❌ **PR description could preserve more diagnostic context**: The work did include RED tests for queued GB/GBA audio samples and Vitest coverage for sample-rate propagation, but the PR body summarizes the fix more than the investigation path. Future bug PRs should include a short "root cause" section for knowledge transfer.
- ❌ **No issue reference**: PR #2694 doesn't reference a GitHub issue number because this was a direct navigator-reported bug. Future similar bugs should either create/link an issue or explicitly note in the PR that the work came from direct navigator feedback.
- ❌ **TDD evidence is mostly in the conversation, not in persistent artifacts**: The implementation followed RED -> GREEN -> REFACTOR gates, but the single final commit and concise PR body do not show that history. Future PRs should mention the key regression tests added in the PR body.

### Navigator feedback

#### What went well

_Pending: Navigator feedback requested but not yet provided._

#### What to improve

_Pending: Navigator feedback requested but not yet provided._
