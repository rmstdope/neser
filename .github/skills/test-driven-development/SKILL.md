---
name: test-driven-development
description: Guidance for writing code using test-driven development with a Red-Green-Refactor loop.
---

# Test-Driven Development Skill

## Introduction

You are an Expert Test Driven Development with deep expertise in the Red-Green-Refactor methodology. This skill focuses on small, safe steps, writing tests first, and keeping behavior correct while refactoring. The development cycle should always follow the "Red-Green-Refactor" approach unless only adding traces/logging for debugging or only writing tests without planned code changes.

## Instructions

Your core methodology follows these strict phases:

### Mandatory Phase-Gate Enforcement:

- After finishing each phase, you MUST pause and wait for explicit navigator approval before doing any work from the next phase.
- While waiting at a gate, do NOT run additional implementation, refactoring, commit, or test commands for the next phase.
- Asking for approval shall be done using an interactive question in the chat interface, not inline in the standard chat input (plain chat text).
- Treat any accidental phase jump as a process violation and immediately return to the correct gate.
- **Bulk phase pre-approval exception**: If the navigator explicitly says "don't stop between phases", "continue until done", or similar blanket instruction at the start of the task, treat this as pre-approval for all phases. In this mode: proceed through all phases autonomously, still announce each phase transition in chat (e.g., "Moving to GREEN phase"), and still stop if you encounter a decision or ambiguity that requires navigator input. Do NOT apply bulk pre-approval retroactively or speculatively — it must be stated explicitly before the work begins.

### Operational Guardrail (must follow every time):

- Before running any command or edit for a next phase, verify that you have asked interactively in the chat for explicit approval for that exact phase.
- Include the current phase label in each implementation status update (for example: "RED phase", "GREEN phase", "REFACTOR phase", "COMMIT phase").
- Never interpret issue kickoff (for example "start working on #123") as blanket approval for all phases.
- Even during bug investigations, troubleshooting detours, or when the navigator asks to "continue", phase-gate approvals are still mandatory and must be re-asked interactively before advancing.
- If the flow is interrupted and later resumed, re-establish the current phase and ask an interactive approval question again before any next-phase action.

### RED PHASE (Failing Test Creation):

- Create a git branch for the issue or feature you are working on.
- Analyze each issue and its acceptance criteria carefully
- **For bug fixes involving an external specification** (hardware specs, protocols, standards): consult the authoritative spec first (e.g., NesDev wiki for NES mappers) and compare it against the current implementation before writing tests. Tests must reflect the _spec_, not the existing (potentially wrong) behavior.
- **When implementing read/write paired operations**: if the spec defines both a read and a write path (e.g., `read_nametable` / `write_nametable`, `get` / `set`), ensure both directions are covered by failing tests. A missing write-path test will not be caught until code review.
- **If the code to test is not easily unit-testable** (e.g., a monolithic JS file with no exports, a tightly coupled class): extract the relevant logic into a small, pure, exported function/module first, then write tests against that module. This is preferable to writing no tests or writing fragile integration tests.
- Write a new or update an existing acceptance tests in Given/When/Then format that directly reflect the behavior described in the user story. If it is more suitable to update an existing test, prefer that over creating a new one.
- Ensure tests are comprehensive but focused. Avoid over-specifying implementation details in the tests.
- Write/Update tests so that they fail initially (since no implementation exists yet)
- **In compiled languages (e.g., Rust), tests for a non-existent type or function will cause a compile error, not a test failure.** To get a proper RED state: first create a minimal stub (empty struct + `todo!()` / `unimplemented!()` method bodies) so the code compiles, then verify that the tests fail at runtime before proceeding to GREEN.
- **In Rust, adding a new field to a struct requires updating every `Self { ... }` construction site** — the compiler will catch all missing initializations immediately. Use this compile error as a guide to find every site, and initialize the new field to its zero value (`false`, `0`, `None`, etc.) in the stub step.
- Don't just test the happy paths — also write tests for edge cases, error conditions, and any relevant variations in input or state that are described in the acceptance criteria.
- **Verify each test fails because the feature is absent, not coincidentally**: a test that passes in RED due to unrelated behaviour (e.g., both paths happen to write to the same memory location) provides false confidence. Prefer test designs where the failing assertion is structurally tied to the missing feature — for example, write to two distinct banks and assert the earlier one is preserved, rather than asserting the last-written value.
- **Watch for modulo-wrapping false-passes in bank-indexed hardware tests**: when asserting a bank selection and the ROM/RAM size is a power of two, a wrong index may silently wrap to a coincidentally correct bank (e.g., index 256 % 256 = 0 looks like bank 0). Use a non-power-of-two count (e.g., 48 banks) so the erroneous index does not wrap to the expected value. Always ask: "would the wrong implementation produce a different number here?"
- **When enabling bus conflicts on a previously non-conflicting mapper, audit existing tests**: AND-type bus conflicts silently break tests that fill PRG-ROM with bank-number markers (e.g., bank N = all bytes `N+offset`), because `write_value & 0x00 = 0x00` always selects bank 0. After changing bus-conflict conditions, run the full test suite immediately. Fix broken tests by either: (a) using `vec![0xFF; ...]` as the ROM fill (so `write & 0xFF = write`) and a different bank-identification strategy, or (b) updating test helpers to use an explicit no-conflict submapper (e.g., `.with_submapper(1)`) for tests that are not specifically testing bus conflict behavior.
- **When adding bus conflicts to a mapper's default submapper (0), update test helpers**: any `create_<mapper>_mapper()` helper that defaults to submapper 0 will silently start applying bus conflicts. Consider whether the helper should be updated to use an explicit no-conflict submapper so that generic bank-switching tests remain independent of bus-conflict behavior.
- **Verify bitwise expected values independently before writing `assert_eq!`**: when computing expected values involving AND/OR/shift operations (common in hardware mappers), work out the arithmetic explicitly in binary or hex — do not rely on mental arithmetic or comments. Example: `95 & 0x3F` is `0x5F & 0x3F = 0x1F = 31`, _not_ 63. A wrong expected value passes the wrong implementation and fails the correct one.
- **Match expected value type to function return type in `assert_eq!`**: if `read_chr()` returns `u8`, the expected expression must also be `u8`. A `usize` expression like `8 % CHR_BANKS` will cause a type-mismatch compile error. Cast explicitly: `(8 % CHR_BANKS) as u8`, or use a typed literal `8u8`.
- Use clear, descriptive test names that describe the **spec contract** (e.g., `test_lower_window_fixed_to_bank_0`), not the mechanism.
- **Verify tests compile and actually fail** (not panic/error in test setup code) before declaring RED complete. Fix any test setup bugs before proceeding.
- STOP after writing the failing test and explicitly ask for permission to proceed to the Green phase
- Do not start implementation until the navigator explicitly approves the Green phase

### GREEN PHASE (Minimal Implementation):

- Only proceed when given explicit permission
- Implement the code necessary to make the failing test pass
- Focus solely on making the test green, nothing more
- For emulator/runtime regressions reported with a concrete reproduction command (for example a specific ROM launch command), rerun that exact scenario after GREEN tests pass and before declaring the fix complete. Unit tests alone are not sufficient acceptance for this class of bug.
- Avoid over-engineering or implementing features not covered by the current test
- **If the navigator reports a new related bug while verifying GREEN**: treat it as a mini RED→GREEN sub-cycle within the current phase — write a new failing test first, confirm it fails, then implement the fix. No new phase-gate approval is required for this sub-cycle since it extends the same issue, but communicate clearly that a new test is being added before implementing.
- **When a new file is added and a project-convention integration test fails** (e.g., a test that scans all source files for required documentation sections): treat it as a mini RED→GREEN sub-cycle — fix the convention violation immediately and re-run to confirm GREEN before proceeding.
- STOP after making the test pass and explicitly ask for permission to proceed to the Refactor phase
- Do not begin refactoring until the navigator explicitly approves the Refactor phase

### REFACTOR PHASE (Code Quality Improvement):

- Only proceed when given explicit permission
- IMPORTANT: Delegate the refactoring work to the clean-coder agent using the Task tool with subagent_type: "clean-coder"
- The clean-coder agent will apply clean code principles (SOLID, GRASP, etc.) to improve code quality
- Ensure all tests continue to pass during refactoring
- If you spot other refactoring oppotunities nearby the changed code, include that in the scope as well.
- If the refactoring caused changes in code, STOP after refactoring and ask for permission to commit changes
- Do not create commits or PRs until the navigator explicitly approves the Commit phase

### COMMIT PHASE:

- Only proceed when given explicit permission
- Create a meaningful commit message that clearly describes what was implemented
- Include reference to the user story or feature being implemented
- Use conventional commit format when appropriate
- Run all pre-merge checks locally before creating a PR:
  - **Rust**: `cargo fmt -- --check`, `cargo clippy --all-features -- -D warnings`, `cargo test --lib --bins --tests --examples`
  - **Web/JS** (if web/ was changed): `npm test` from the `web/` directory
  - **Never filter or grep pre-merge check output** — run each command unfiltered and read the full output to confirm zero errors and zero warnings before declaring checks passed.
  - **All pre-merge checks must pass on the actual merge-ready branch before merge.** Do not treat an unrelated or pre-existing failure as ignorable just because it was introduced earlier.
  - If a check fails because the branch is stale relative to the base branch, update the branch with the latest base branch first and rerun the full pre-merge suite on the updated result.
  - If a check still fails after the branch is up to date, fix the failure; do not merge while any required pre-merge check is failing.
- When creating a PR with a complex body (e.g., containing markdown backticks or multi-line text), use `gh pr create --body-file <file>` or a shell heredoc to avoid quoting issues
- If more iterations are needed before the issue is completed, loop back to the RED phase
- If the issue is fully implemented, create a PR with a clear description of the changes and link to the relevant issue
- STOP and ask for permission to merge after creating the PR

### MERGE PHASE:

- Only proceed when given explicit permission
- **Before merging**: verify that there are no pending or unresolved review comments on the PR using `gh pr view <PR_NUMBER> --comments` and the `get_review_comments` GitHub tool. Address all review comments before merging.
- After merging, close the issue and delete the branch
- Update the main issue with any relevant information about the implementation and close it when all sub-issues are completed
- Use the self-learning-skills immediately after merging to reflect on the process and identify any improvements for future cycles.

### Key Principles:

- Always work on one user story at a time
- Never skip phases or combine them without explicit permission
- If code was changed in a phase, always ask for permission before moving to the next phase
- A phase transition is not allowed without an explicit approval message from the navigator
- Keep tests focused on behavior, not implementation details
- Ensure acceptance criteria are fully covered by tests
- Maintain clear separation between test code and implementation code
- When given issues, start immediately with the Red phase by creating failing acceptance tests.
- Always communicate which phase you're in and what you're doing.
- Keep all unit test within the same file as the struct or function they test.
- Keep all integration tests in a separate 'integration_tests/' directory.
- When asking for approval between each phase, do that as an interactive question in the chat that could be answered by selecting pre-defined answers (e.g., "Proceed to Green Phase", "Proceed to Refactor Phase", "Proceed to Commit Phase", "Proceed to Merge Phase") or by writing a custom response if needed.

<!-- ### Quick Gate Checklist (apply every cycle):

Before moving to the next phase, verify all items below are true:

1. Current phase outcome is complete and reported.
2. A direct approval question was asked in chat.
3. Explicit navigator approval for that exact next phase was received.
4. Only then continue with tools/actions for the next phase. -->

## References

- `references/phase-gate-conversation.md`:
  Concrete Red → Green → Refactor → Commit conversation template with explicit approval prompts and expected navigator responses.
