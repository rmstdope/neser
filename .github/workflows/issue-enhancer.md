---
name: Issue Enhancer
description: Automatically enhances an issue by ensuring correct labeling and by applying github-issue-designer quality principles for outcome-oriented issue design.
engine: copilot
on:
  issues:
    types: [opened]
  reaction: "eyes"
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
    allowed: [enhanced]
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
- **Issue Labels**: ${{ toJson(github.event.issue.labels.*.name) }}
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
3. Set a new descriptive title if the current title is not sufficiently descriptive of the issue outcome.
4. Plan to add the `enhanced` label at workflow completion using the safe-outputs configuration. Do not add any additional labels in this workflow.
5. Evaluate and improve the issue content using `github-issue-designer` principles:
   - Keep one clear, independently deliverable outcome
   - Make scope explicit and minimal
   - Add out-of-scope/non-goals when needed to prevent scope creep
   - Ensure acceptance criteria are objective and testable
   - Ensure validation steps are concrete and mapped to outcomes
6. Ensure the issue body follows the recommended structure from `github-issue-designer` when applicable:
   - Summary
   - Problem
   - Scope
   - Out of scope
   - Acceptance criteria
   - Validation
   - Dependencies / Links
7. If the issue is already high quality, preserve the author intent and only apply minimal edits.
8. Preserve existing links, code blocks, issue/PR references, and technical identifiers exactly unless they are clearly incorrect.

When you improve the issue description, emit an `update_issue` safe output for the triggering issue with `operation: replace` and include the full improved issue body content.

After completing enhancement decisions for any issue without the `enhanced` label (including when no body update is needed), emit exactly one `add_labels` with `labels: ["enhanced"]`.
