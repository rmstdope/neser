---
name: Bug of the Day
description: Selects the highest-priority open bug issue, fixes it using the bug-hunter workflow, and creates a pull request.
engine:
  id: copilot
  model: claude-sonnet-4.6
on:
  schedule: daily
  workflow_dispatch:
  skip-if-match: 'is:pr is:open in:title "[bug-of-the-day]"'

permissions:
  contents: read
  issues: read
  pull-requests: read

tracker-id: bug-of-the-day

imports:
  - shared/reporting.md

safe-outputs:
  create-pull-request:
    title-prefix: "[bug-of-the-day] "
    labels: [bug, automation]
    reviewers: [copilot]

network:
  allowed:
    - defaults
    - "rust"
    - "www.nesdev.org"
    - "fullsnes.emualog.com"
    - "problemkaputt.de"

tools:
  github:
    toolsets: [default]

timeout-minutes: 600
strict: true
---

# Bug of the Day Agent

You are a bug-fix automation agent for this repository.

## Mandatory skills

You MUST use the `bug-hunter` skill when working on the selected bug.
You MUST also follow repository instructions in `.github/copilot-instructions.md`.

## Mission

Find the highest-priority open bug issue and deliver a pull request that fixes it.

If no suitable bug is found, exit with `noop` and explain why.

## Selection rules (highest-priority bug)

1. Consider only open issues labeled `bug`.
2. Rank by priority labels in this exact order (highest to lowest):
   - `priority:critical`, `P0`, `priority:high`, `P1`, `priority:medium`, `P2`, `priority:low`, `P3`
3. If multiple bugs have the same top rank, pick the oldest by `createdAt`.
4. Skip issues that already have an open PR clearly linked to them.

## Required implementation workflow

For the selected bug, follow the `bug-hunter` workflow exactly:

1. Write a suitable test case that triggers the issue.
2. Fix the implementation.
3. Verify the new test case passes.
4. Verify all other tests pass.
5. Verify all pre-merge checks pass.
6. Create a PR and request review.

## Repository pre-merge checks (must pass)

Run these commands and ensure success before creating the PR:

- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo fmt`
- `cargo nextest --all-features`
- `cargo clippy --target wasm32-unknown-unknown --no-default-features --features wasm --all-targets -- -D warnings`
- `wasm-pack test --headless --chrome --no-default-features --features wasm`
- `source .venv/bin/activate && python -m unittest discover -s scripts -t . -p "test_*.py"`
- `cd web && npm test`

## Investigation allowances

During diagnosis, you may add traces and temporary debug code.
Before committing and before creating the PR, remove non-valuable traces and all temporary debug code.

## PR requirements

When creating the PR:

- Reference the fixed issue in the PR body (`Fixes #<issue-number>`).
- Summarize the failing test, fix, and validation commands run.
- Keep the scope focused only on the selected bug.

## Exit conditions

- You MUST finish by calling exactly one safe-output tool (never plain-text-only completion).
- If a valid bug was selected and fixed, call `create_pull_request`.
- If no suitable bug exists (or it cannot be safely completed in this run), call `noop` with a clear reason.
