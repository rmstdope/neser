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

### Operational Guardrail (must follow every time):

- Before running any command or edit for a next phase, verify that you have asked interactively in the chat for explicit approval for that exact phase.
- Include the current phase label in each implementation status update (for example: "RED phase", "GREEN phase", "REFACTOR phase", "COMMIT phase").
- Never interpret issue kickoff (for example "start working on #123") as blanket approval for all phases.
- Even during bug investigations, troubleshooting detours, or when the navigator asks to "continue", phase-gate approvals are still mandatory and must be re-asked interactively before advancing.
- If the flow is interrupted and later resumed, re-establish the current phase and ask an interactive approval question again before any next-phase action.

### RED PHASE (Failing Test Creation):

- Create a git branch for the issue or feature you are working on.
- Analyze each issue and its acceptance criteria carefully
- **For bug fixes involving an external specification** (hardware specs, protocols, standards): consult the authoritative spec first (e.g., NesDev wiki for NES mappers) and compare it against the current implementation before writing tests. Tests must reflect the *spec*, not the existing (potentially wrong) behavior.
- Write a new or update an existing acceptance tests in Given/When/Then format that directly reflect the behavior described in the user story. If it is more suitable to update an existing test, prefer that over creating a new one.
- Ensure tests are comprehensive but focused. Avoid over-specifying implementation details in the tests.
- Write/Update tests so that they fail initially (since no implementation exists yet)
- Use clear, descriptive test names that describe the **spec contract** (e.g., `test_lower_window_fixed_to_bank_0`), not the mechanism.
- **Verify tests compile and actually fail** (not panic/error in test setup code) before declaring RED complete. Fix any test setup bugs before proceeding.
- STOP after writing the failing test and explicitly ask for permission to proceed to the Green phase
- Do not start implementation until the navigator explicitly approves the Green phase

### GREEN PHASE (Minimal Implementation):

- Only proceed when given explicit permission
- Implement the code necessary to make the failing test pass
- Focus solely on making the test green, nothing more
- Avoid over-engineering or implementing features not covered by the current test
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
- Run all pre-merge checks locally before creating a PR: `cargo fmt -- --check`, `cargo clippy --all-features`, and the full test suite (`cargo test --lib --bins --tests --examples`). Fix any failures before proceeding.
- If more iterations are needed before the issue is completed, loop back to the RED phase
- If the issue is fully implemented, create a PR with a clear description of the changes and link to the relevant issue
- STOP and ask for permission to merge after creating the PR

### MERGE PHASE:

- Only proceed when given explicit permission
- **Before merging**: verify that all CI checks on the PR have passed (not just local tests) using `gh pr checks <PR_NUMBER>`. Do not merge if any check is failing or still pending.
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
