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
   - **ALWAYS use `--body-file` for multi-line content** - Inline `--body` with multiple lines can corrupt shell state and cause parsing errors.
   - For multi-line bodies, create a temporary file with heredoc:

```sh
cat > /tmp/issue-body.md << 'EOF'
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
   - Use double quotes around `--title` arguments with spaces.
   - For `--body`, always use `--body-file` instead of inline content.
   - Escape embedded double quotes in titles with a backslash: `\"`.
   - For strings with apostrophes in titles, use double quotes (not single quotes).
   - Avoid special characters in temporary file paths when using heredoc.

8. **Verify and fix**:

Always verify issue creation and content after running `gh issue create`:

```sh
# View newly created issue
gh issue view <issue-number>

# Or list recent issues to confirm creation
gh issue list --state open --limit 5 --json number,title
```

If formatting or content is incorrect, update the issue:

```sh
gh issue edit <issue-number> --body-file /tmp/corrected-body.md
```

9. **Troubleshooting - Shell state corruption**:

If terminal shows `dquote>` prompt or similar after `gh` commands:

- The heredoc or quoting process corrupted shell state.
- **Solution**: Run simple command like `echo "test"` to reset state, or open a new terminal.
- **Prevention**: Always wrap heredoc content in single quotes: `<<'EOF'` (not `<<EOF`)
- **Alternative**: Use Python subprocess for complex cases:

```python
import subprocess
result = subprocess.run([
    "gh", "issue", "create",
    "--title", "My title",
    "--body", body_content,
    "--label", "label1,label2"
], capture_output=True, text=True)
print(result.stdout)
```

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
