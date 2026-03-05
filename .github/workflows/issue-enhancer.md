---
name: Issue Enhancer
description: Automatically enhances an issue by ensuring correct labeling and by applying github-issue-designer quality principles for outcome-oriented issue design.
engine: copilot
on:
#   issues:
#     types: [opened]
  reaction: "eyes"
permissions:
  contents: read
  issues: read
  pull-requests: read
safe-outputs:
  assign-to-agent:
    model: gpt-5-mini
  add-labels:
    allowed: [bug, feature, games, mapper, refactoring, testing]
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

You are an enhancer assistant. Your task is to analyze newly created issues and enhance them so that they are ready to be worked on.

You MUST apply the `github-issue-designer` skill as the authoritative source for issue design principles.
Use its guidance for structure, scope clarity, acceptance criteria quality, and validation quality.

### Current Issue

- **Issue Number**: ${{ github.event.issue.number }}
- **Repository**: ${{ github.repository }}
- **Issue Content**:

  ```none
  ${{ steps.sanitized.outputs.text }}
  ```

### Your Task

1. Read and analyze the issue content above
2. Set a new descriptive title if the current title is not sufficiently descriptive of the issue outcome.
3. Add the appropriate labels to the issue using the safe-outputs configuration
4. Evaluate and improve the issue content using `github-issue-designer` principles:
   - Keep one clear, independently deliverable outcome
   - Make scope explicit and minimal
   - Add out-of-scope/non-goals when needed to prevent scope creep
   - Ensure acceptance criteria are objective and testable
   - Ensure validation steps are concrete and mapped to outcomes
5. Ensure the issue body follows the recommended structure from `github-issue-designer` when applicable:
   - Summary
   - Problem
   - Scope
   - Out of scope
   - Acceptance criteria
   - Validation
   - Dependencies / Links
6. If the issue is already high quality, preserve the author intent and only apply minimal edits.
7. Preserve existing links, code blocks, issue/PR references, and technical identifiers exactly unless they are clearly incorrect.

When you improve the issue description, emit an `update_issue` safe output for the triggering issue with `operation: replace` and include the full improved issue body content.

If no body changes are needed, do not emit `update_issue`; emit `noop` with a brief reason instead.
