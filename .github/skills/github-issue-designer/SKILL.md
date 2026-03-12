---
name: github-issue-designer
description: Guidance for designing high-quality GitHub issues, including scope, structure, acceptance criteria, and effective issue updates.
---

# GitHub Issue Designer Skill

## Purpose

Use this skill whenever an issue should be created or updated.

This skill defines issue content quality:

- what to write in an issue
- how to structure it
- how to keep scope clear
- how to make acceptance criteria verifiable

Technical `gh` execution is handled by `github-administration`.

## Mandatory Usage

Always use `github-issue-designer` whenever an issue should be created or updated.

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

## Sub-issue Guidelines

When splitting larger work:

- prefix titles with `Sub-issue (<parent-issue-number>):`
- include parent linkage in body
- ensure each sub-issue has independent acceptance criteria
- try to link sub-issues to its parent using the gh extension `issue-child-add`, usage `gh issue-child-add <parent> <child>`

## Update Guidelines

When updating an existing issue:

- record scope changes explicitly
- keep progress updates concise and factual
- adjust acceptance criteria when requirements change
- maintain links to related PRs/tests
- if feedback indicates ambiguity, add explicit test vectors and expected visual outcomes to the issue
- if CRC/golden checks are used, include an artifact review step (screenshots or equivalent) before accepting CRC values

## Label Intent Guidance

Choose labels by issue intent:

- `bug` = defects
- `enhancement` = new capability
- `refactoring` = internal code quality work
- `testing` = test-focused scope
- `games` / `mapper` = domain-focused scope
- `enhanced` = issue content was created or updated with AI assistance

(`github-administration` applies labels technically.)

## Pairing Rule

For issue creation/update workflows:

- use `github-issue-designer` for content design
- use `github-administration` for `gh` command execution

## Retrospective Requirement

After issue creation and merge/close workflows, run `self-learning-skills` retrospective and refine this skill when recurring quality gaps are found.
