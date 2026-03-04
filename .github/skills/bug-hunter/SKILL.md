---
name: bug-hunter
description: Workflow for fixing bugs safely by reproducing with tests first, applying minimal fixes, validating all checks, and creating a PR for review.
---

# Bug Hunter Skill

## Introduction

Use this skill when fixing bugs. The primary goal is to make bug fixes reliable, reviewable, and low-risk by following a strict test-first workflow and verifying all project checks before opening a PR.

This skill complements `test-driven-development` and should be applied in small, independently shippable increments.

## Mandatory Workflow

Execute the steps below in order:

1. **Write a suitable test case that triggers the issue**
  - Reproduce the bug with a focused test that fails for the right reason.
  - Prefer the smallest test scope that still captures the real bug behavior.
  - If possible, encode the expected behavior using authoritative specification references (for this repo, see NesDev docs when relevant).

2. **Fix the implementation**
  - Implement the minimal change needed to satisfy the failing test.
  - Avoid broad refactors unless they are required to complete the bug fix safely.

3. **Verify that the new test case is now PASS**
  - Re-run the newly added/updated test(s) first.
  - Confirm the test now passes and directly validates the fix.

4. **Verify that all other test cases PASS**
  - Run the relevant wider suites for impacted areas.
  - Then run the full regression suite required by the repository before merge.

5. **Verify that all pre-merge checks PASS**
  - Run all required pre-merge checks defined by repository policy.
  - Do not claim completion unless each required check is green.

6. **Create PR and ask for review**
  - Create a clear PR describing the failing behavior, test added, fix applied, and validation performed.
  - Request navigator review explicitly before merge.

## Allowed During Investigation

While diagnosing and narrowing down bugs, the practitioner may:

- Add traces/logging/instrumentation.
- Add temporary debug code.

## Mandatory Cleanup Before Commit

Before committing or opening a PR:

- Remove any trace/logging lines that are not valuable for future debugging.
- Remove all temporary debug code used only during investigation.
- Re-run the relevant tests/checks after cleanup to ensure behavior is unchanged.

## Quality Rules

- Keep changes focused to the bug scope.
- Prefer simple design and minimal-risk fixes.
- Ensure tests document intended behavior and prevent regression.
- Never merge without review (four-eye principle).

## Completion Checklist

- [ ] Bug reproduced by a failing automated test.
- [ ] Implementation fix applied.
- [ ] New bug test is passing.
- [ ] Existing tests are passing.
- [ ] All required pre-merge checks are passing.
- [ ] PR created and review requested.
- [ ] Non-valuable traces removed.
- [ ] Temporary debug code removed.
