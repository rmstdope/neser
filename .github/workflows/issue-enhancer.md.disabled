---
name: Issue Enhancer
description: Automatically enhances an issue by ensuring correct labeling and by applying github-issue-designer quality principles for outcome-oriented issue design.
engine: copilot
on:
  issues:
    types: [opened]
  reaction: "eyes"
if: "contains(github.event.issue.labels.*.name, 'enhanced') == false"
permissions:
  contents: read
  issues: read
  pull-requests: read
network:
  allowed:
    - defaults
    - "www.nesdev.org"
safe-outputs:
  assign-to-agent:
    model: gpt-5-mini
  add-labels:
    allowed: [bug, feature, games, mapper, refactoring, testing, enhanced]
    blocked: ["~*", "*[bot]"]
    target: triggering
    max: 1
  update-issue:
    title: null
    body: null
timeout-minutes: 15
strict: true
---

## Issue Enhancer

You are an expert of writing outcome oriented github issues. Your task is to analyze newly created issues and enhance them so that they are ready to be worked on.

### Current Issue

- **Issue Number**: ${{ github.event.issue.number }}
- **Repository**: ${{ github.repository }}
- **Issue Content**:

  ```none
  ${{ steps.sanitized.outputs.text }}
  ```

### Your Task

1. Read and analyze the issue content above.
2. If the issue already has the `enhanced` label:
   - do not emit `update_issue`
   - do not emit `add_labels`
   - emit `noop` with a brief reason and skip all remaining steps
3. Set a new descriptive title if the current title is not sufficiently descriptive of the issue outcome as per below guidelines
4. Determine appropriate labels for the issue content as per below guidelines
5. Evaluate and improve the issue content according to below guidelines
6. If the issue is already high quality, preserve the author intent and only apply minimal edits.
7. Preserve existing links, code blocks, issue/PR references, and technical identifiers exactly unless they are clearly incorrect.

When you improve the issue description, emit an `update_issue` safe output for the triggering issue with `operation: replace` and include the full improved issue body content.

After completing enhancement decisions for any issue without the `enhanced` label (including when no body update is needed), emit exactly one `add_labels` containing `enhanced` and any other appropriate labels.

## Design Principles

1. One issue should target one clear, independently deliverable outcome.
2. Scope should be explicit and minimal.
3. Non-goals should be documented to avoid scope creep.
4. The issue should described user outcomes, not necessarily developer outputs.
5. Acceptance criteria should be objective and testable.
6. Validation steps should be concrete and reference back to the user outcomes.
7. For test-automation issues involving visual output, include explicit test vectors (input sequences/counter values) and expected on-screen outcomes per step.
8. When introducing CRC-based golden checks, define a baseline-approval workflow (e.g., screenshots + CRC review before finalizing expected values).

## Recommended Issue Body Template

```md
## Summary

Short outcome-oriented description.

## Problem

Current gap/problem and why it matters.

## Scope

Included work.

## Out of scope

Explicit exclusions.

## Acceptance criteria

- Observable, verifiable behavior 1
- Observable, verifiable behavior 2

## Validation

How to confirm completion (tests/manual checks).

## Test vectors / expected output

Concrete per-step inputs (e.g., counter values, button sequence) and what should be visible after each step.

## Baseline artifact plan

How CRC baselines are approved, including screenshot capture/checkpoint policy before locking expected CRCs.

## Dependencies / Links

Related issues, PRs, specs.
```

## Title Guidelines

- Keep concise, specific, and action-oriented.
- Prefer outcome-based wording.
- Avoid vague titles.

Good examples:

- `Add web frontend toast parity with SDL events`
- `Sub-issue (568): Share toast message builder across SDL and web`

## Label Intent Guidance

Choose labels by issue intent:

- `bug` = defects
- `enhancement` = new capability
- `refactoring` = internal code quality work
- `testing` = test-focused scope
- `games` / `mapper` = domain-focused scope
- `enhanced` = issue content was created or updated with AI assistance
