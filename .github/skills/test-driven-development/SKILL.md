---
name: test-driven-development
description: Guidance for writing code using test-driven development with a Red-Green-Refactor loop.
---

# Test-Driven Development Skill

## Introduction

Use this skill when implementing or changing code with TDD. It focuses on small, safe steps, writing tests first, and keeping behavior correct while refactoring. The development cycle should always follow the "Red-Green-Refactor" approach unless only adding traces/logging for debugging or only writing tests without planned code changes.

## Instructions

1. **Clarify the goal**: Identify the smallest behavior change or feature to implement.
2. **Work in small increments**: Each step should be independently shippable and low risk.
3. **Red**: Write failing tests that capture the desired behavior (black box and white box where appropriate).
4. **Prove failure**: Run tests and confirm they fail for the right reason.
5. **Pause for review on complex tasks**: After red, request navigator review before moving on.
6. **Green**: Implement the minimal code to make the tests pass.
7. **Prove success**: Run relevant tests and confirm both new and existing tests pass before requesting approval.
8. **Pause for review**: After green, request navigator review before refactoring.
9. **Refactor**: Improve design and readability without changing behavior. Keep refactors small.
10. **Review refactor**: If refactor changes code, request navigator review before proceeding; if no code changes, you can continue.
11. **Bug fixes**: Tracing is allowed early, but add or update a test that reproduces the bug before the fix.
12. **Stay TDD**: Do not stray from Red-Green-Refactor unless only adding traces/logging.
13. **Test strategy**: Use unit tests (black box and white box) and integration tests when end-to-end behavior matters, preferably aligned with specs.
14. **Repeat**: Move to the next smallest behavior and iterate.
15. **Pre-merge checks**: Run the full regression suite before merging any code changes.

## Examples

- Input: "Add a new command parser rule."
  Output: "Write a test for the new rule, make it fail, implement just enough parsing logic, then refactor for clarity."
- Input: "Fix a regression in save state loading."
  Output: "Add a test reproducing the regression, confirm failure, fix minimal code, then clean up."
