---
name: github-issue-administration
description: Guidance for creating and updating GitHub issues with the gh CLI, including safe quoting practices.
---

# GitHub Issue Administration Skill

## Introduction

Use this skill when creating, updating, or managing GitHub issues. It covers issue writing, workflow updates, and safe `gh` command usage with reliable quoting.

## Instructions

1. **Clarify the change**: Determine whether you need a new issue, a sub-issue, or an update to an existing issue.
2. **Write clear issues**:
   - Title: short, action-oriented, and specific.
   - Body: problem statement, expected behavior, scope, and acceptance criteria.
   - Add relevant labels (bug, enhancement, games, mapper, refactoring, testing).
3. **Sub-issues**: For large work, split into smaller issues and prefix the title with `Sub-issue (<parent-issue-number>):`.
4. **Assign owner**: Always assign the issue to the developer doing the work.
5. **Update during workflow**:
   - Add progress notes when major steps are completed.
   - Link related PRs and reference test results when available.
   - Close the issue after the PR is merged.
6. **Use `gh` safely**:
   - Avoid backticks in the issue text.
   - Only create or update one issue per command to minimize quoting complexity.
   - Prefer `--title` and `--body` with double quotes; use single quotes only for short strings without apostrophes.
   - For multi-line bodies, use a heredoc to avoid escaping:

```sh
cat <<'EOF' > /tmp/issue-body.md
Problem:
- ...

Scope:
- ...

Acceptance criteria:
- ...
EOF

gh issue create --title "Short, specific title" --body-file /tmp/issue-body.md --label enhancement
```

7. **Quoting rules**:
   - Use double quotes around arguments with spaces.
   - Escape embedded double quotes with a backslash: `\"`.
   - For strings with apostrophes, use double quotes (not single quotes).
   - Prefer `--body-file` for complex text.

8. **Verify**:

After creating or updating an issue, read it back using `gh issue view <issue-number>` to ensure formatting and content are correct. If it is not, update the issue with `gh issue edit <issue-number>` to fix any formatting issues.

## Examples

- Create issue:

```sh
gh issue create --title "Fix save state corruption" --body "Problem: ..." --label bug
```

- Create sub-issue:

```sh
gh issue create --title "Sub-issue (123): Add APU envelope tests" --body-file /tmp/issue-body.md --label testing
```

- Update issue:

```sh
gh issue comment 123 --body "Progress: tests added; implementation in progress."
```
