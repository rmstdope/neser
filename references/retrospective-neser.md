# Retrospective — neser

Structured retrospective entries for AI-assisted workflows on the neser project.
Each entry captures what went well, what to improve, and which skills were used.

---

## 2026-06-23 - #2831 / PR #2866: Split native keyboard.rs into hotkeys/dispatch/mapping + merge

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2866
**Linked issues:** #2831 (epic #2825, item I1.5)

### Customizations used

| Type | Name | Purpose |
| --- | --- | --- |
| Prompt/instruction | `[[PLAN]]` mode + "interview relentlessly" directive | Resolved the design forks (minimal-move vs extract-and-separate, module map, test placement) before coding. |
| Instruction | Repo custom instructions (TDD, four-eye, incremental, validation gate, `gh`) | Governed the small-increment loop and the issue/branch/PR/merge workflow. |
| Skill (implicit) | `rust-code-refactoring` / `clean-coder` | Guided the behavior-preserving file split into cohesive `hotkeys`/`console_keyboard`/`controller_mapping` modules. |
| Skill (implicit) | `test-driven-development` | Used the existing 88 keyboard tests as the safety net validated after each extraction. |
| Skill | `self-learning-skills` | Captured this post-merge retrospective entry. |

### What went well

- **Minimal-move decision paid off:** Confirming "relocate whole free functions, no body restructuring" up front made every extraction a pure move — public entry points stayed in `mod.rs`, so the `keyboard::{...}` paths needed **no re-exports** (simpler than the #2830 config split).
- **Incremental extract→build→test→commit loop:** Moving one submodule at a time (controller_mapping → hotkeys → console_keyboard) with the 88-test safety net caught a script bug immediately (the hotkeys extraction over-grabbed 4 lines of `handle_gameboy_key_pressed`'s doc comment).
- **Section-based test redistribution:** The pre-existing `// ──` section comments grouped tests by domain, so the ~1,400-line test block split cleanly into modules; `cargo fix --lib --tests` removed the resulting unused imports in one pass.
- **Disciplined CI handling:** When `web-integration` failed, I proved it was a flaky, newly-added SNES **web** Playwright test unrelated to a native-Rust change (same web build passed earlier; failed/passed inconsistently on its own PR #2823), documented the evidence on the PR, and did **not** override a red check — it went green on retry and was merged legitimately.

### What to improve

- **Range-based extraction is fragile:** The doc-comment over-grab happened because a hard-coded line range straddled an adjacent item. Prefer anchoring on item boundaries (function signature / matching brace) over absolute line numbers when scripting moves, and always diff the extracted file for stray leading/trailing lines.
- **Native frontend tests aren't in the default `test-dir.sh` path:** `./scripts/test-dir.sh src/frontends` runs `--no-default-features`, which excludes the `native` feature → 0 keyboard tests. Validate native frontend changes with default/`--features native` (e.g. `cargo test --lib frontends::native`) and note this in the PR.
- **Flaky `web-integration` (snes-frontend-flow) costs cycles:** The pause/resume Playwright test (`BrokenPipeError`) failed ~5× before passing. Worth quarantining/stabilizing it separately so unrelated PRs aren't blocked.
- **Shared-env disk pressure recurred:** `target/` hit 100% again mid-task; pruning `target/**/incremental` (safe) unblocked it. Consider doing this proactively on long build-heavy sessions.

### Navigator feedback

- Directives: "address the review comments, then ensure ci runs green and then merge" and (when unavailable) "work autonomously and make good decisions; if unresolvable, stop and summarize." Addressed both Copilot doc-accuracy comments (`handle_controller_key` P2 routing, `handle_common_hotkey` F1/F2/F3), pursued genuine green CI rather than overriding the flaky red check, and merged once all checks passed.

---

## 2026-06-24 — PR #2894: Sub-issue (2724): Add shared SNES ROM integration runner and baseline helpers

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2894
**Linked issues:** #2872, #2724

### Customizations used

| Type | Name | Purpose |
| --- | --- | --- |
| Skill | `github-issue-designer` | Helped identify #2872 as a dependency-free next slice of #2724. |
| Skill | `test-driven-development` | Guided the generated pass/fail/timeout ROM fixtures and runner behavior tests. |
| Skill | `rust-developer` | Supported idiomatic Rust implementation of the shared SNES runner and test-only probes. |
| Skill | `snes-hardware-research` | Grounded LoROM fixtures, WRAM marker `$7E1FF0`, idle-loop PCs, and SNES header fixes. |
| Skill | `self-learning-skills` | Supported capturing this retrospective after merge. |
| Agent | `rubber-duck reviewer` | Helped review assumptions and reason through CI/review feedback. |
| Prompt | `autopilot plan mode` | Supported autonomous planning, execution, validation, and merge follow-through. |
| Instructions | `repository custom instructions` | Applied the repo's TDD, validation, review, and documentation expectations. |

### What went well

- **Dependency-aware issue choice:** Planning selected a dependency-free #2724 sub-issue (#2872), avoiding blocked follow-up suite work before the shared runner existed.
- **Spec-grounded generated fixtures:** The SNES hardware research + TDD pairing produced in-Rust LoROM pass/fail/timeout fixtures with corrected header offsets, WRAM markers, idle-loop PC detection, tick/frame budgets, and screen CRC diagnostics without adding external ROM assets.
- **CI feedback improved the branch:** CI exposed stale native SNES fixture header offsets that local no-default-feature runs missed; fixing them made the test fixtures consistent across native, web, WASM, and the new runner.
- **Review feedback tightened API semantics:** The Copilot review comment improved `RunConfig` by making `max_ticks == 0` disable the tick budget, matching the existing zero-frame-budget behavior.

### What to improve

- **Centralize generated SNES test ROM helpers:** Stale fixture header offsets existed in several frontend test helpers. Future SNES test work should prefer a shared helper or explicit invariant tests so generated ROM headers cannot drift.
- **Define edge-case runner semantics during RED:** The initial TDD pass covered pass/fail/timeout and frame limits, but not zero/unlimited tick semantics. Future runner APIs should define and test zero/unlimited budget behavior up front.
- **Record active customizations as they are invoked:** The retrospective had to reconstruct the skill/agent list after merge. During future autopilot work, keep the active customization list current as the workflow proceeds.
- **Document fixture conventions close to code:** Future SNES test authors should not need to rediscover the marker address, idle-loop PCs, and header-relative offsets; keep those conventions visible near the fixture builder.

### Navigator feedback

Feedback pending; navigator was unavailable during the post-merge retrospective.

---

## 2026-06-23 — PR #2865: Add SNES web frontend flow coverage

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2865
**Linked issues:** #2823

### Customizations used

| Type | Name | Purpose |
| --- | --- | --- |
| Skill | `test-driven-development` | Kept the SNES frontend work anchored in test-first slices: ROM support assertion, browser flow coverage, then edge-case tightening. |
| Skill | `snes-hardware-research` | Helped with SNES ROM fixture expectations and minimal header/extension assumptions for the browser test helper. |
| Instructions | `copilot-instructions.md` | Applied the repo workflow rules for incremental changes, review/CI discipline, and retrospective capture. |

### What went well

- ✅ The work was split into a fast unit-level ROM extension assertion and a Playwright flow spec, giving both quick regression signal and end-to-end coverage.
- ✅ Tightening the tests with case-insensitive and negative SNES cases, plus extracting a shared SNES ROM helper, reduced duplication and made the coverage easier to extend.
- ✅ The Playwright flow was stabilized before merge, which is the right call for browser coverage that otherwise tends to be CI-flaky.

### What to improve

- ❌ When adding a new supported console in `rom_extensions`, include the case-insensitive and unsupported-extension cases in the first pass so the coverage matrix is complete before review.
- ❌ The browser-flow spec needed a later flake-reduction pass; front-load a reusable wait/stability helper for Playwright frontend tests so timing assumptions don’t leak into the first version.
- ❌ If review comments or CI reruns are part of the workflow, record the exact `gh`/verification sequence in the PR notes so future frontend test work can reuse the stabilization steps without rediscovery.

### Navigator feedback

#### What went well

No additional feedback.

#### What to improve

No additional feedback.

---

## 2026-06-23 - #2830 / PR #2863: Split platform/config.rs into per-domain modules + merge

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2863
**Linked issues:** #2830 (epic #2825, item I1.4)

### Customizations used

| Type | Name | Purpose |
| --- | --- | --- |
| Prompt/instruction | `[[PLAN]]` mode + "interview relentlessly" directive | Drove a design-tree interview that resolved every fork (per-domain vs functional split, decomposition pattern, test placement, flag-table location, re-export strategy) before any code changed. |
| Instruction | Repo custom instructions (TDD, four-eye, incremental, validation gate, `gh` conventions) | Governed the small-increment loop, the pre-merge checkpoint, and the issue/branch/PR workflow. |
| Skill (implicit) | `rust-code-refactoring` / `clean-coder` | Guided behavior-preserving decomposition into cohesive per-domain modules with thin orchestrators. |
| Skill (implicit) | `test-driven-development` | Used the existing 339-test platform suite as the safety net validated after every extraction. |
| Skill | `self-learning-skills` | Captured this post-merge retrospective entry. |

### What went well

- **Front-loaded design interview:** Resolving all major forks up front (per-domain split, "handled-bool" config-value dispatch, autorun↔ram-init coupling kept in the orchestrator, central flag table to protect help-text ordering, explicit re-exports) meant the implementation was mechanical with no mid-flight redesign.
- **Codebase-grounded recommendations:** Decisions were anchored in evidence — the just-merged NES split (#2860) as precedent, `grep` counts of external symbol usage to set the exact re-export list, and the 24 `FrontendConfig` struct-literal sites that fixed the "keep it flat" constraint.
- **Tight increment loop:** "extract one domain → build → `test-dir.sh src/platform` → commit" caught issues immediately (an audio white-box test referencing moved constants; an unused `Config` import after test redistribution) while keeping each commit independently green (339 tests throughout).
- **Behavior preservation discipline:** The one real cross-domain dependency (autorun forcing zero-init RAM) was preserved by passing the captured `--ram-init-mode` value into `autorun::apply_args`; the central flag table eliminated help-text ordering risk.
- **Full gate before PR:** clippy `-D warnings`, fmt, 11,898 lib tests, wasm (87), Python (346), and npm (411) all green prior to opening the PR.

### What to improve

- **Explicit skill invocation:** The repo instruction says to explicitly announce which skills are in use; this task applied refactoring/TDD principles but never invoked or named a skill in-chat. For refactors, explicitly engage `rust-code-refactoring` / `test-driven-development` up front.
- **Self-review of touched doc comments:** A pre-existing `--display` doc inaccuracy on `apply_args` was only caught by the automated reviewer. When reorganizing code, do a dedicated pass over the doc comments on the moved/edited items.
- **Error-precedence nuance left untested:** Grouping `apply_args` by domain changes which error wins when *multiple* flags are simultaneously malformed (untested, still rejected). Either preserve exact statement order or add a test documenting the chosen precedence.
- **Shared-environment disk hygiene:** `target/` reached 100% disk mid-task and interrupted the wasm test; proactively prune `target/**/incremental` (safe, regenerates) on long build-heavy tasks in shared environments.

### Navigator feedback

- Directive: "address the review comments, ensure ci runs green and then merge." The navigator reviewed, authorized the merge after the Copilot review comment was addressed and CI was confirmed green (four-eye principle satisfied), and the PR was merged with the branch deleted and epic #2825 updated.

---

## 2026-06-22 - #2807 / PR #2815: Super Scope light gun controller implementation + merge

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2815
**Linked issues:** #2807

### Customizations used

| Type | Name | Purpose |
| --- | --- | --- |
| Skill | `test-driven-development` | Guided edge-triggered action handling (turbo toggle, trigger/pause locking) via unit tests for serial bit sequence. |
| Skill | `snes-hardware-research` | Informed implementation of fullsnes Super Scope serial protocol and bit-field ordering. |
| Skill | `rust-developer` | Guided state machine design with latch-processed edge detection and counter saturation. |
| Skill | `github-administration` | Managed issue workflow, PR reviews, and merge coordination via `gh`. |
| Skill | `self-learning-skills` | Captured this post-merge retrospective entry. |

### What went well

- **State machine clarity:** The turbo-toggle locking pattern (`if turbo_pressed && !turbo_lock`) avoids accidental re-triggers on held button, and the edge-triggered per-strobe processing (`latch_processed` flag) mirrors proven patterns from standard controller implementations.
- **Test-driven design:** The unit test `serial_sequence_matches_the_documented_field_order` validates both the bit ordering (trigger → cursor → turbo_enabled → pause → filler bits → offscreen) and the interaction of offscreen state with trigger output, catching early if position/state synchronization breaks.
- **State persistence:** The `capture_state`/`restore_state` methods follow established patterns for save-state support, correctly mapping all ephemeral state (locks, toggle state) and positional data.
- **Hardware accuracy:** The implementation correctly handles the documented Super Scope protocol—offscreen suppresses trigger output at bit 0, turbo gating affects trigger output, and cursor/pause/offscreen are independent bit fields.

### What to improve

- **Missing position validation tests:** While the code correctly bounds offscreen detection (`x < 0 || y < 0 || x >= 256 || y >= 224`), no unit tests verify edge cases (x/y at boundary transitions, negative coordinates). Add explicit boundary tests to the existing test suite.
- **No integration test for actual game behavior:** The unit test covers protocol compliance in isolation; an integration test running a Super Scope game ROM (e.g., *Super Scope 6* or *Terminator 2*) would validate that the emulator correctly interprets the serial stream in full context. This is a pattern issue across input device tests.
- **Assumption about counter saturation:** The code uses `saturating_add(1)` on the counter, which gracefully prevents overflow. However, there's no explicit test documenting when/how the counter should saturate (if ever during normal 8-bit reads). Document or test this assumption.
- **No documentation on turbo timing:** The turbo logic updates state on every read cycle when turbo is pressed; real hardware turbo is typically a timer-based oscillation (e.g., every N frames). The current approach may not match hardware timing if games depend on specific turbo frequency.

### Navigator feedback

#### What went well

Effective workflow guidance. Communication and decision-making were very efficient.

#### What to improve

No additional feedback provided.

### Skill Recommendations for Update

1. **test-driven-development** — Add a guideline: "For input device state machines, prioritize edge-triggered action tests (locking, toggle toggles, strobe synchronization) before testing data fields. This catches subtle state-machine bugs early."

2. **snes-hardware-research** — Create a checklist for light gun / pointer input devices: verify (a) position bounding/offscreen detection, (b) button locking/edge-trigger patterns, (c) turbo oscillation frequency if applicable, (d) integration with auto-joypad read timing.

3. **Controller/Input Implementation Pattern** — Consider creating a new optional guidance document (`src/snes/input/IMPLEMENTATION_PATTERNS.md`) capturing:
   - Standard state-machine patterns for controllers (edge triggers, locking, per-strobe processing).
   - Checklist for save-state support (capture/restore completeness, ephemeral vs. persistent state).
   - Testing strategy: unit tests for protocol/serial order, integration tests for actual game ROMs.
   - This could reduce friction for future input devices (e.g., trackball, paddle, justifier).

---

## 2026-06-20 - #2774 / PR #2786: SNES APU bootstrap review-fix, CI green, merge

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2786
**Linked issues:** #2774 (Sub-issue of #2721)

### Customizations used

| Type | Name | Purpose |
| --- | --- | --- |
| Skill | `github-administration` | Drove safe review-thread handling, CI polling via `gh`, and merge/issue-close operations. |
| Skill | `test-driven-development` | Guided RED -> GREEN fix for legacy save-state compatibility behavior. |
| Skill | `self-learning-skills` | Triggered post-merge retrospective capture. |
| Agent | `rubber-duck` | Reviewed changed file for correctness and edge cases in restore-path behavior. |
| Agent | `Iteration Retrospective Gatherer` | Attempted structured retrospective capture and highlighted missing feedback metadata. |

### What went well

- The review comment was converted into a focused regression test first, then a minimal code fix in `SnesApu::restore_state`.
- Review hygiene stayed tight: thread reply posted with concrete commit reference and thread confirmed resolved before merge.
- Merge gate discipline held: waited for full check rollup to complete with successful conclusions before merging.

### What to improve

- Rebase churn occurred because the PR branch advanced during review-fix work. For active PRs, fetch/rebase immediately before committing review fixes to reduce conflict cycles.
- The retrospective helper required additional metadata/feedback. Keep a small standard template ready so retrospective capture is never blocked by tooling assumptions.

### Navigator feedback

Navigator unavailable; feedback pending.

---

## 2026-06-18 - #2747 / PR #2756: SNES battery SRAM persistence review-fix + merge

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2756
**Linked issues:** #2747 (Sub-issue of #2719)

### Customizations used

| Type | Name | Purpose |
| --- | --- | --- |
| Skill | `rust-developer` | Guided focused Rust changes around API additions (`sram_size`) and behavior-safe load/save handling. |
| Skill | `test-driven-development` | Used a RED test for mismatched `.sav` sizing, then GREEN implementation to satisfy acceptance behavior. |
| Skill | `github-administration` | Structured PR review-thread handling, replies/resolution, CI polling, and merge execution via `gh`. |
| Skill | `self-learning-skills` | Captured this post-merge retrospective entry. |

### What went well

- Review comments were translated into concrete, minimal fixes with direct tests: `.sav` size compatibility checks, `tempfile::tempdir()` isolation, helper reuse, and naming clarity.
- Thread hygiene was strong: each inline review comment was replied to and all threads were resolved before merge.
- CI discipline held: merge happened only after the full check rollup reached completed/successful state.

### What to improve

- The original implementation drifted from one acceptance detail (size-compatible load only). For persistence features, add explicit size-compat tests in the first RED slice, not as follow-up.
- The TDD phase-gate skill conflicts with autopilot execution. Future sessions should explicitly acknowledge autopilot pre-approval at kickoff to avoid process friction.
- Keep `sav_path()` as the single source for `.sav` derivation from the start to avoid temporary dead-code suppression and duplicate path logic.

### Navigator feedback

Navigator unavailable; feedback pending.

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
*** End of File

---

## 2026-07-03 — PR #2929: Fix SPC SMP edge arithmetic

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2929
**Linked issues:** none

### Customizations used

| Type         | Name                         | Purpose                                                                 |
| ------------ | ---------------------------- | ----------------------------------------------------------------------- |
| Skill        | `bug-hunter`                 | Drove reproduction of the longer-running `spc_smp.sfc` frame-2200 `Failed 02` signal and focused the diagnostic loop. |
| Skill        | `rust-developer`             | Supported targeted Rust fixes for SPC MMIO and timer behavior without unrelated production-code changes. |
| Skill        | `test-driven-development`    | Kept the fix anchored to reproduced ROM failure signals, final golden updates, and local/CI verification. |
| Skill        | `snes-hardware-research`     | Grounded SPC MMIO and timer behavior decisions in SNES/APU hardware semantics. |
| Skill        | `self-learning-skills`       | Captured this post-merge AI-customization retrospective entry. |
| Instructions | `.github/copilot-instructions.md` | Applied repository expectations for explicit skill usage, TDD, validation gates, review/merge discipline, and retrospective capture. |
| Other        | `Copilot review comments`    | Provided AI review feedback that was addressed before final validation and merge. |

### What went well

- ✅ Extending the `bug-hunter` loop to the longer-running frame-2200 `spc_smp.sfc` failure avoided stopping at an earlier partial success and exposed the remaining `Failed 02` behavior before merge.
- ✅ The `snes-hardware-research` + `rust-developer` pairing kept the production-code fix focused on SPC MMIO and timer semantics instead of broad emulator churn.
- ✅ The `test-driven-development` workflow converted the reproduced ROM failure into updated final goldens and then verified the outcome through local gates and green CI before merge.
- ✅ Copilot review comments were treated as part of the AI verification loop: they were addressed before the final local/CI validation rather than after the branch was already considered done.

### What to improve

- ❌ The earlier PR-created retrospective recorded a frame-900 golden, but the merge work later found a frame-2200 failure. Future AI-assisted ROM debugging should include a longer-run acceptance checkpoint before considering SPC/APU timing fixes complete.
- ❌ The exact diagnostic sequence from frame-2200 `Failed 02` to the final MMIO/timer fix was only summarized post-hoc. Future bug-hunting runs should preserve a compact symptom → hypothesis → register/timer behavior → verification trace in the work notes.
- ❌ Prompt and agent usage were not explicitly recorded in the handoff. Future retrospectives should state "no prompts used" and "no agents used" when true so customization absence does not need to be inferred.
- ❌ Final golden updates are durable project knowledge; future TDD handoffs should record the exact verification command(s), failing frame/CRC, and final passing frame/CRC alongside the golden changes.

### Navigator feedback

#### What went well

No feedback provided (navigator unavailable/pending)

#### What to improve

No feedback provided (navigator unavailable/pending)

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

## 2026-05-27 — PR #2694: Fix web handheld audio sample drops

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

---

## Iteration: 65816 CPU spec-compliance round  PR #27372 

**Date:** 2026-06-17
**Branch:** `2729-65816-addressing-modes-and-opcodes`
**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2737
**Linked issues:** #2729

### Customizations used

| Type         | Name                        | Purpose                                                                                           |
| ------------ | --------------------------- | ------------------------------------------------------------------------------------------------- |
COMMIT cycle for all 8 spec-compliance fixes.                      |
| Skill        | `snes-hardware-research`    | Grounded edge-case fixes (decimal V flag, DP page wrapping, MVN/MVP) against WDC spec.           |
| Instructions | `copilot-instructions.md`   | Applied repository workflow rules: TDD, phase gates, pre-merge checks, issue tracking.           |

### What went well

-  **Sub-agent spec review surfaced 8 non-trivial bugs**: A dedicated post-implementation sub-agent review pass against the WDC 65C816 spec found WDM panic, immediate-mode cycles, stack cycles, abs-idx X=0 cycles, RTI native cycles, SBC decimal V flag, DP emulation-mode wrapping, and MVN/MVP per-byte execution. These would not have been caught by functional opcode tests alone.
-  **TDD kept a massive opcode table tractable**: Red-Green-Refactor-Commit discipline across 256 opcodes and 8 follow-up fixes prevented the "implement everything, test later" drift that commonly causes regressions on large opcode tables.
COMMIT progression while still stopping at  a good balance between pace and oversight.MERGE 
-  **Comprehensive pre-merge gate caught an additional issue**: Clippy revealed the unreachable `_` arm in the dispatch table (confirming all 256 opcodes were covered) and the unused TestBus in non-test builds. Both were clean, valuable signals from a full gate run.
-  **Code review agent correctly validated subtle formula**: The refactor-phase reviewer suggested removing `!` from the SBC V-flag overflow formula, but manual verification showed the current formula (`!(a ^ not_op) & (a ^ bin)`) was  the reviewer's suggestion would have broken the tests. Phase-gate discipline meant the incorrect suggestion was caught before any change was made.correct 

### What to improve

 **Sub-agent review should be a mid-implementation checkpoint, not only post-implementation**: The 8 fixes were found after the full 256-opcode implementation. For work packages this large, a first review pass at the halfway mark (e.g., after addressing modes, before opcode dispatch is complete) would surface architectural corrections while implementation context is still fresh.- 
 **MVN/MVP per-byte execution is a design assumption, not a detail**: The fix (one byte transfer per `step()` call, PC held at instruction) implies the original loop-all-bytes design was architecturally wrong. Future block-move opcodes in any new CPU core should start with a per-byte test first. Add this as a checklist item in the SNES CPU development skill.- 
 **No coverage topology summary in the PR**: 11,200 tests passing is strong, but for a 256-opcode core there is no easy way to confirm every opcode has at least one cycle-count test, one flag test, and one addressing-mode test. Future large opcode PRs should include a brief coverage table or note in the PR body.- 

### Navigator feedback

No additional feedback provided beyond driver observations.

---

## 2026-06-17 — PR #2755: SNES HDMA per-scanline transfers (#2746)

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2755
**Linked issues:** #2746 (Sub-issue of #2719)

### Customizations used

| Type | Name | Purpose |
| --- | --- | --- |
| Skill | `snes-hardware-research` | Researched fullsnes, anomie specs for HDMA descriptor semantics, cycle accounting, direct/indirect table behavior |
| Skill | `test-driven-development` | RED-GREEN-REFACTOR approach: 9 unit tests written first, minimum implementation to pass, refactored with review |
| Skill | `rust-developer` | Core Rust patterns: state management in DmaController, error handling, encapsulation of HDMA runtime state |
| Instructions | SNES development practices | Four-eye principle, pre-merge checkpoints (clippy, fmt, targeted tests, full suite), TDD discipline, small increments |

### What went well

- ✅ **Hardware-research-first approach prevented design mismatches**: Starting with fullsnes/anomie specs via `snes-hardware-research` skill ensured the explicit HDMA timing model (18-cycle init + 8 per channel + 16 per indirect pointer load) was correct from the start, avoiding downstream timing bugs.
- ✅ **Explicit test-friendly APIs for pre-PPU features**: The `hdma_init()/hdma_do_line()` design on SnesSystemBus decoupled testing from PPU scheduling logic (#2720 deferred). This pattern enables independent HDMA validation before PPU wiring is complete, and should be reused for other timing-sensitive features.
- ✅ **Cycle accounting tests caught implementation bugs**: Adding cycle-delta assertions in unit tests (e.g., `assert_eq!(ticks, expected)`) caught the indirect-terminator bug early (descriptor 00 incorrectly charging +16 pointer load cycles). This pattern should be standard for all timing-critical emulator code.
- ✅ **TDD + explicit specification semantics**: Writing descriptor-parsing and state-transition tests before implementation naturally exposed edge cases (descriptor 0x80 as non-repeat 128-line, repeat-mode semantics, channel 0→7 ordering). 9 unit tests cover init behavior, per-line state, direct/indirect transfers, and cycle accounting.
- ✅ **All CI checks passing before merge**: Pre-merge validation gates (cargo clippy, fmt, targeted SNES tests, full lib/wasm/Python/JS tests) ran green, reducing post-merge regression risk.

### What to improve

- ❌ **Design-rationale documentation is sparse**: The choice of explicit APIs over auto-scheduling in PPU is correct, but the rationale isn't captured in code comments. A "Design Decisions" section in PR or a comment block in system_bus.rs explaining why PPU wiring is deferred would help future maintainers understand the architecture.
- ❌ **HDMA runtime state lacks lifecycle documentation**: The per-channel arrays (hdma_repeat_mode[], hdma_lines_left[], hdma_do_transfer[]) are initialized in hdma_init(), mutated in hdma_do_line(), and cleared at frame end, but no comment explains this state machine. Future HDMA work (e.g., mid-frame enable, per-channel pause) should include a state-transition table.
- ❌ **$420C latch vs. hdma_active_mask distinction is implicit**: The implementation uses $420C as a write/read latch (spec-compliant) and hdma_active_mask for per-frame tracking (implementation choice to allow channels to terminate mid-frame independently). A short comment in system_bus.rs or architecture.md explaining this split would prevent future confusion about which field to read.

### Navigator feedback

**What went well (confirmed by navigator):**
- Hardware-research-first approach was highly effective for getting timing model right
- TDD with cycle-delta assertions caught subtle bugs naturally
- Explicit test-friendly APIs are a reusable pattern for future timing-sensitive features
- Pre-merge validation gates as a quality barrier are reliable

**What to improve (confirmed by navigator):**
- Design rationale for explicit APIs (vs. auto-scheduling) should be documented in code comments or PR description for future reference
- HDMA state-machine lifecycle deserves a comment block or brief architecture diagram
- The $420C latch vs. hdma_active_mask distinction should be documented to prevent future misunderstandings

---

## 2026-06-25 — PR #2902: Sub-issue (2825): Add a shared Stateful save-state trait and platform save_state helpers

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2902
**Linked issues:** #2834 (sub-issue of epic #2825, item I2.1)

### Customizations used

| Type | Name | Purpose |
| --- | --- | --- |
| Prompt | `plan-issue` (`[[PLAN]]` mode) | Relentless one-question-at-a-time design interview with recommended answers; resolved ~12 design decisions before any code was written. |
| Instructions | `copilot-instructions.md` | Enforced the small-increment TDD loop, four-eye / no-auto-merge, question-UI collaboration, `gh` workflow, and the mandated retrospective. |
| Skill | `test-driven-development` | Drove Red→Green→Refactor→Commit across 5 increments (platform foundation + NES / GB / GBA / SNES). |
| Skill | `rust-developer` | Shaped the `Stateful` trait design, hand-rolled error types, and naming. |
| Agent | `rubber-duck` | Acted as the REFACTOR-phase reviewer on every increment. |
| Skill | `self-learning-skills` | Captured this retrospective entry. |

### What went well

- ✅ **Up-front bulk pre-approval composed cleanly with TDD:** Authorizing "work autonomously through RED→GREEN→REFACTOR→COMMIT; stop only at design decisions and MERGE" kept a fast pace while pinning human oversight to exactly the two points that matter — design forks and the merge gate.
- ✅ **`plan-issue` interview front-loaded the design forks:** Resolving ~12 decisions before coding (trait placement/fallibility, shared `SaveStateError` shape, thiserror-vs-hand-rolled, `IncompatibleVersion { found, supported }`, GB/GBA enum removal, fallible-restore handling, one-PR / per-console-commit delivery) produced near-zero design churn during implementation.
- ✅ **`rubber-duck` caught real pre-commit defects every REFACTOR:** It flagged untracked golden fixtures that would break a clean checkout, a missing `Error::source()` chain, and a stale doc link to a removed type — all before they reached a commit.
- ✅ **`rust-developer` yielded one uniform trait/error pattern across four consoles:** The shared shape made the per-console commits read as mechanical, low-risk repetitions rather than four bespoke implementations.

### What to improve

- ❌ **`test-driven-development` lacks a compiled-language refactor RED pattern:** For pure rename / trait-adoption refactors, the natural RED is a compile failure referencing the not-yet-existing type/impl — distinct from the `unimplemented!()` stub used for brand-new functions. Add a short "RED for refactors in compiled languages" note to the skill.
- ❌ **Missing golden-fixture size guidance forced a mid-task round-trip:** The real NES save-state fixture was ~430 KB (framebuffer dominates); gzip compression (`flate2` dev-dep) brought fixtures down to ~2.4–24 KB. Add a "real console save-states are large — compress golden fixtures" heads-up to the relevant testing / save-state guidance.
- ❌ **Recurring disk-space hazard ("No space left on device"):** Mid-task the build filled the disk and required pruning `target/` (incremental + wasm target). This repeats a prior retrospective finding — add a standing note to proactively prune `target/**/incremental` and the wasm target on long build-heavy sessions.
- ❌ **Non-obvious design choices aren't auto-surfaced for reviewers:** The trait-method → private `*_inner` delegation (chosen for uniformity/safety over moving large method bodies) warrants a one-line rationale. Suggest `rust-developer` / `clean-coder` emit a brief rationale note when introducing non-obvious delegation.

### Navigator feedback

_AI-observed retrospective: the navigator was running in autopilot this iteration and did not add feedback. The "What went well" / "What to improve" sections above are driver observations; a human can append navigator notes below later._

#### What went well

_Pending — no navigator feedback this iteration._

#### What to improve

_Pending — no navigator feedback this iteration._

---

## 2026-07-02 — PR #2926: Fix SNES SPC timer target-counter behavior

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2926
**Linked issues:** #2913

### Customizations used

| Type         | Name                         | Purpose                                                                 |
| ------------ | ---------------------------- | ----------------------------------------------------------------------- |
| Skill        | `rust-developer`             | Supported focused Rust changes for SPC timer behavior and test assets.  |
| Skill        | `test-driven-development`    | Anchored the fix in the failing `spc_mem_access_times.sfc` ROM signal.  |
| Skill        | `snes-hardware-research`     | Grounded SPC timer target-counter semantics in SNES/APU behavior.       |
| Skill        | `self-learning-skills`       | Captured this customization-focused retrospective entry.                 |
| Instructions | `copilot-instructions.md`    | Applied repository TDD, validation, documentation, and retrospective expectations. |
| Other        | `Copilot review comments`    | Provided AI review feedback that was addressed before final validation.  |

### What went well

- ✅ Using `spc_mem_access_times.sfc` as the acceptance signal kept the SPC timer target-counter fix tied to observable emulator behavior, and promoting golden CRC `0x3AC3E30F` turns that discovery into reusable regression coverage.
- ✅ The `snes-hardware-research` + `test-driven-development` pairing was well matched for a timing-sensitive APU bug: hardware semantics guided the expected behavior while the ROM result proved the fix.
- ✅ Copilot review comments gave a focused AI-review checkpoint after implementation; addressing those comments before rerunning the local gates kept the AI feedback in the main verification loop instead of treating it as optional cleanup.
- ✅ Updating the SNES README and ROM manifest in the same work package made the newly promoted verification ROM discoverable for future AI-assisted SNES test work.

### What to improve

- ❌ The retrospective context only had a post-hoc skills list. Future iterations should announce active skills at kickoff and record explicitly when no prompts or agents are used, so customization usage does not need reconstruction.
- ❌ For hardware timing fixes, capture the exact source-backed rule that drove the implementation in the active work notes or test rationale; this reduces future AI review ambiguity around SPC timer target-counter edge cases.
- ❌ When AI review comments are resolved, keep a compact comment-to-resolution note in the PR or work log so future retrospectives can identify which Copilot feedback changed the final patch and which was only confirmatory.
- ❌ When promoting a golden CRC, record the pre-fix failure signal and final verification command sequence in the AI work log so later retrospectives can distinguish the effective diagnostic path from the final success state.

### Navigator feedback

#### What went well

No feedback provided

#### What to improve

No feedback provided

---

## 2026-07-02 — PR #2928: Promote SPC timer ROM

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2928
**Linked issues:** none

### Customizations used

| Type         | Name                         | Purpose                                                                 |
| ------------ | ---------------------------- | ----------------------------------------------------------------------- |
| Skill        | `rust-developer`             | Supported safe Rust-repository changes limited to ROM verification metadata and documentation. |
| Skill        | `test-driven-development`    | Kept the promotion anchored to the observed PASS screen and CRC `0x249738B2` signal. |
| Skill        | `self-learning-skills`       | Captured this customization-focused retrospective entry after PR creation. |
| Instructions | `copilot-instructions.md`    | Applied repository expectations for TDD, validation, documentation updates, and retrospectives. |

### What went well

- ✅ The workflow treated the formerly ignored blargg `spc_timer.sfc` as the acceptance signal: verifying the PASS screen and CRC `0x249738B2` before unignoring/promoting it made the catalog update evidence-backed rather than a bookkeeping-only change.
- ✅ The `test-driven-development` skill fit a validation-only PR well because the outcome was still framed as a reproducible regression signal: an ignored ROM now becomes part of the verified SPC/APU safety net.
- ✅ Updating the verified SPC/APU catalog, README/manifest pass-fail counts, and stale timer-speed CRC documentation in the same package reduced the chance that future AI-assisted ROM work would see contradictory verification metadata.
- ✅ Keeping the PR to promotion/documentation work after the preceding SPC timer behavior fix avoided unnecessary production-code churn and made the scope clear for review.

### What to improve

- ❌ The need to correct stale timer-speed CRC docs shows that CRC references can drift across documents. Future ROM-promotion iterations should include an explicit workspace search for the ROM name and old/new CRC before finalizing metadata.
- ❌ Promotion touched several bookkeeping locations. Add or follow a compact ROM-promotion checklist in the active work notes: unignore entry, verified catalog, README counts, manifest counts, and any stale CRC documentation.
- ❌ The customization context was provided post-hoc as a skills list. Future iterations should announce active skills at kickoff and explicitly record whether prompts or agents were not used, so retrospectives do not have to infer absence.
- ❌ For validation-only PRs, record the exact verification command and observed pre/post status in the AI work log alongside the final CRC; that would make the diagnostic path as reusable as the final catalog entry.

### Navigator feedback

#### What went well

No feedback provided

#### What to improve

No feedback provided

---

## 2026-07-03 — PR #2928: Promote SPC timer ROM

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2928
**Linked issues:** none

### Customizations used

| Type         | Name                         | Purpose                                                                 |
| ------------ | ---------------------------- | ----------------------------------------------------------------------- |
| Skill        | `rust-developer`             | Supported safe repository edits limited to verified ROM metadata and documentation. |
| Skill        | `test-driven-development`    | Kept the promotion grounded in a reproducible PASS screen and CRC `0x249738B2` signal. |
| Skill        | `self-learning-skills`       | Captured this AI-customization-focused post-merge retrospective entry. |
| Instructions | `copilot-instructions.md`    | Applied repository expectations for TDD, validation gates, review/merge discipline, and retrospectives. |

### What went well

- ✅ The AI workflow treated the ignored blargg `spc_timer.sfc` ROM as evidence first: PASS screen plus CRC `0x249738B2` was verified before promoting it to the verified test set.
- ✅ The `test-driven-development` customization translated cleanly to a validation-only change by making the promoted ROM the regression signal, rather than forcing unnecessary production-code edits.
- ✅ The `rust-developer` customization helped keep the patch scope narrow: README-SNES and manifest/catalog corrections were made without touching emulator production code.
- ✅ Repository instructions kept the final loop explicit: documentation/manifest consistency was checked, CI was confirmed green, and the retrospective was run after merge.

### What to improve

- ❌ The AI customization context still depended on a post-hoc skills list. Future iterations should record active skills at kickoff and explicitly note when no prompts or agents are used.
- ❌ The exact local verification command and observed output for the PASS/CRC evidence were not included in the retrospective context. Future validation-only PRs should preserve that command transcript in the AI work log.
- ❌ ROM promotion touched multiple documentation/catalog locations. Future AI-assisted ROM promotions should use a short active checklist covering ignored status, verified catalog, README counts, manifest counts, and stale CRC references.
- ❌ PR metadata reconstruction is easier when branch name and linked-issue status are captured before merge. Future retrospectives should include those fields in the handoff summary, even when linked issues are `none`.

### Navigator feedback

#### What went well

No feedback provided

#### What to improve

No feedback provided

---

## 2026-07-03 — PR #2929: Fix SPC SMP edge arithmetic

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2929
**Linked issues:** none

### Customizations used

| Type         | Name                         | Purpose                                                                 |
| ------------ | ---------------------------- | ----------------------------------------------------------------------- |
| Skill        | `bug-hunter`                 | Guided reproduction of the longer-timeout `spc_smp.sfc` failure and narrowed the failing signal to MMIO readback behavior. |
| Skill        | `rust-developer`             | Supported targeted Rust changes for SPC MMIO register semantics without broad production-code churn. |
| Skill        | `test-driven-development`    | Kept the fix anchored to the failing frame-900 ROM signal and the updated golden CRC `0x73C335D6`. |
| Skill        | `snes-hardware-research`     | Grounded SPC MMIO edge behavior for `$F0/$F1`, `$F2/$F3`, and `$F8/$F9` in SNES/APU semantics. |
| Skill        | `self-learning-skills`       | Captured this AI-customization-focused retrospective entry after PR creation. |
| Instructions | `.github/copilot-instructions.md` | Applied repository expectations for explicit skills usage, TDD, validation, and retrospective capture. |

### What went well

- ✅ The `bug-hunter` workflow was well matched to the failure: reproducing the longer-timeout `spc_smp.sfc` failure at frame 900 with CRC `0x1A6CB67C` / `Failed 02` gave the AI a concrete before-signal instead of relying on speculative SPC changes.
- ✅ Pairing `snes-hardware-research` with `rust-developer` kept the fix narrow and hardware-specific: write-only `$F0/$F1` reads, full-byte `$F2` DSPADDR readback, 7-bit `$F3` masking/read-only mirror behavior, and `$F8/$F9` AUXIO readback were handled as MMIO semantics rather than generalized emulator cleanup.
- ✅ The `test-driven-development` customization translated the ROM failure into durable regression coverage by updating the golden to frame 900 CRC `0x73C335D6` after the behavior fix.
- ✅ Including `self-learning-skills` in the handoff made customization usage explicit enough to avoid reconstructing which skills drove the work package after the PR was created.

### What to improve

- ❌ The PR context provided the final failing and passing CRCs, but not the exact diagnostic sequence that isolated `$F0/$F1`, `$F2/$F3`, and `$F8/$F9`. Future AI bug-hunting iterations should preserve a compact trace from symptom to culprit register group so later work can reuse the path, not only the result.
- ❌ The title, branch, and skills were available post-hoc, but prompt/agent absence was not explicitly recorded. Future handoffs should state "no prompts used" and "no agents used" when true, so retrospectives do not infer missing customization categories.
- ❌ Hardware edge-case fixes benefit from source-backed rule snippets. Future `snes-hardware-research` use should capture the exact SPC MMIO read/write rules in the active work notes or test rationale, especially when different registers intentionally use different readback masks.
- ❌ The longer-timeout ROM failure was central to the diagnosis. Future AI-assisted ROM debugging should record timeout/frame choices as part of the reproducible acceptance setup so a later run knows why frame 900 was selected.

### Navigator feedback

#### What went well

No feedback provided

#### What to improve

No feedback provided

---

## 2026-07-03 — PR #2929: Fix SPC SMP edge arithmetic

**Repository:** rmstdope/neser
**PR URL:** https://github.com/rmstdope/neser/pull/2929
**Linked issues:** none

### Customizations used

| Type         | Name                         | Purpose                                                                 |
| ------------ | ---------------------------- | ----------------------------------------------------------------------- |
| Skill        | `bug-hunter`                 | Drove reproduction of the longer-running `spc_smp.sfc` frame-2200 `Failed 02` signal and focused the diagnostic loop. |
| Skill        | `rust-developer`             | Supported targeted Rust fixes for SPC MMIO and timer behavior without unrelated production-code changes. |
| Skill        | `test-driven-development`    | Kept the fix anchored to reproduced ROM failure signals, final golden updates, and local/CI verification. |
| Skill        | `snes-hardware-research`     | Grounded SPC MMIO and timer behavior decisions in SNES/APU hardware semantics. |
| Skill        | `self-learning-skills`       | Captured this post-merge AI-customization retrospective entry. |
| Instructions | `.github/copilot-instructions.md` | Applied repository expectations for explicit skill usage, TDD, validation gates, review/merge discipline, and retrospective capture. |
| Other        | `Copilot review comments`    | Provided AI review feedback that was addressed before final validation and merge. |

### What went well

- ✅ Extending the `bug-hunter` loop to the longer-running frame-2200 `spc_smp.sfc` failure avoided stopping at an earlier partial success and exposed the remaining `Failed 02` behavior before merge.
- ✅ The `snes-hardware-research` + `rust-developer` pairing kept the production-code fix focused on SPC MMIO and timer semantics instead of broad emulator churn.
- ✅ The `test-driven-development` workflow converted the reproduced ROM failure into updated final goldens and then verified the outcome through local gates and green CI before merge.
- ✅ Copilot review comments were treated as part of the AI verification loop: they were addressed before the final local/CI validation rather than after the branch was already considered done.

### What to improve

- ❌ The earlier PR-created retrospective recorded a frame-900 golden, but the merge work later found a frame-2200 failure. Future AI-assisted ROM debugging should include a longer-run acceptance checkpoint before considering SPC/APU timing fixes complete.
- ❌ The exact diagnostic sequence from frame-2200 `Failed 02` to the final MMIO/timer fix was only summarized post-hoc. Future bug-hunting runs should preserve a compact symptom → hypothesis → register/timer behavior → verification trace in the work notes.
- ❌ Prompt and agent usage were not explicitly recorded in the handoff. Future retrospectives should state "no prompts used" and "no agents used" when true so customization absence does not need to be inferred.
- ❌ Final golden updates are durable project knowledge; future TDD handoffs should record the exact verification command(s), failing frame/CRC, and final passing frame/CRC alongside the golden changes.

### Navigator feedback

#### What went well

No feedback provided (navigator unavailable/pending)

#### What to improve

No feedback provided (navigator unavailable/pending)
