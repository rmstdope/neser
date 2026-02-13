---
name: github-administration
description: Guidance for creating and updating GitHub issues and pull requests with the gh CLI, including safe quoting practices.
---

# GitHub Administration Skill

## Introduction

Use this skill when creating, updating, or managing GitHub issues and pull requests. It covers issue and PR writing, workflow updates, safe `gh` command usage with reliable quoting, and the complete lifecycle from issue creation through branch management to PR merge.

## Instructions

1. **Clarify the change**: Determine whether you need a new issue, a sub-issue, or an update to an existing issue.
2. **Write clear issues**:
   - Title: short, action-oriented, and specific.
   - Body: problem statement, expected behavior, scope, and acceptance criteria.
   - Add relevant labels (bug, enhancement, games, mapper, refactoring, testing).
   - Assign issue: Use `--assignee @me` to assign to yourself, or `--assignee <username>` for others.
3. **Sub-issues**: For large work, split into smaller issues and prefix the title with `Sub-issue (<parent-issue-number>):`.
4. **Assign owner**: Always assign the issue using `--assignee @me` (for yourself) or `--assignee <username>` when creating the issue - don't leave it unassigned.
5. **Update during workflow**:
   - Add progress notes when major steps are completed.
   - Link related PRs and reference test results when available.
   - Close the issue after the PR is merged.
6. **Use `gh` safely**:
   - Avoid backticks in the issue text.
   - Only create or update one issue per command to minimize quoting complexity.
   - **ALWAYS use `--body-file` for multi-line content** - Inline `--body` with multiple lines can corrupt shell state and cause parsing errors.
   - For multi-line bodies, choose one of these safe methods:

### Method A: IDE File Creation (PREFERRED - No shell quoting issues)

Use your IDE/development environment's file creation capabilities:

```
1. Write issue body to /tmp/issue-body.md (e.g., via create_file tool)
2. Run: gh issue create --title "Short title" --body-file /tmp/issue-body.md --label label
3. No shell quoting complexity, completely safe
```

### Method B: Bash Heredoc (Requires careful quoting)

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

**Important:** Always use single quotes `<<'EOF'` to prevent variable expansion and escape processing.

7. **Quoting rules**:
   - Use double quotes around `--title` arguments with spaces.
   - For issue `--body`, **always use `--body-file`** instead of inline content (even for short bodies).
   - Create body files using IDE file creation tools (preferred) to avoid all shell quoting issues.
   - If using bash heredoc, always wrap in single quotes: `<<'EOF'` (not `<<EOF`).
   - Escape embedded double quotes in titles with a backslash: `\"`.
   - For strings with apostrophes in titles, use double quotes (not single quotes).

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

## Troubleshooting

### Preflight checks (recommended)

Before creating issues, run a quick preflight to reduce failures and rework:

```sh
# Confirm authenticated account (assignee context)
gh api user --jq .login

# Confirm available labels in this repository
gh label list --json name --limit 100

# Optional: confirm current repo context
gh repo view --json nameWithOwner
```

This helps avoid common errors like missing labels or assigning to the wrong account.

### PR creation prompts for where to push branch

**Problem:** When running `gh pr create`, you get an interactive prompt asking "Where should we push the '{branch-name}' branch?"

**Cause:** The feature branch exists locally but hasn't been pushed to the remote repository yet.

**Solution:**

```sh
# Push the branch first
git push origin your-branch-name

# Then create the PR
gh pr create --title "..." --body "..." --head your-branch-name
```

**Prevention:** Always use the `--head` flag when creating a PR to explicitly specify the branch:

```sh
# This won't prompt if branch doesn't exist, but tells you to push first
gh pr create --title "..." --body "..." --head branch-that-needs-pushing
```

### Shell state corruption from gh commands

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

### Label not found error

If you get error: `could not add label: 'X' not found`:

- The label doesn't exist in the repository
- **Solution 1**: Use a different label that exists (check with `gh label list`)
- **Solution 2**: Create the label in GitHub UI first, then retry issue creation
- **Prevention**: Validate labels exist before batch creation:
  ```sh
  gh label list --json name --limit 50 | grep '"name":'
  ```
- **Recovery**: If some issues fail due to label error:
  1. Create the missing label in GitHub UI
  2. Edit failed issues to add the label: `gh issue edit <number> --label "new-label"`
  3. Or recreate the issues with correct labels

## Examples

### Preferred: Creating issues with body files (IDE-based)

For multi-issue creation, use IDE file creation tools:

```
1. Create body files:
   /tmp/issue1.md - First issue body
   /tmp/issue2.md - Second issue body
   /tmp/issue3.md - Third issue body

2. Create issues:
   gh issue create --title "Issue 1 title" --body-file /tmp/issue1.md --label enhancement --assignee @me
   gh issue create --title "Issue 2 title" --body-file /tmp/issue2.md --label refactoring --assignee @me
   gh issue create --title "Issue 3 title" --body-file /tmp/issue3.md --label testing --assignee @me

3. Verify:
   gh issue list --state open --limit 5 --json number,title,assignees
```

**Advantages:** No shell quoting issues, supports unlimited issue body length, clear separation of concerns, tracks issue assignments.

### Creating pull requests

When creating a PR after work is complete on a feature branch:

**IMPORTANT: Always push the branch before creating the PR** to avoid interactive prompts:

```sh
# Step 1: Ensure branch is pushed to remote
git push origin your-branch-name

# Step 2: Create the PR with explicit branch reference
gh pr create \
  --title "Fix #492: Brief description of the fix" \
  --body "Your PR body can be inline for PRs (unlike issues).

## Changes
- What was changed
- Additional improvements

## Testing
- How it was tested
- Test coverage details

Fixes #492" \
  --head your-branch-name
```

**PR body guidelines:**

- Can use inline content (unlike issues where multi-line should use --body-file)
- Use clear section headers: `## Changes`, `## Testing`, `## Fixes`
- Reference the issue with `Fixes #123` which will auto-link and close the issue on merge
- Keep formatting clear and scannable

**Verification after PR creation:**

```sh
# View the created PR
gh pr view <pr-number>

# Or list recent open PRs
gh pr list --state open --limit 5 --json number,title
```

### Batch creation for large grouped initiatives

When creating many related issues (e.g. Phase 1, 2, 3 recommendations from code review):

```
1. Create all body files first (e.g., phase1_issue1.md through phase3_issue5.md)

2. Create issues in priority order:

   # Phase 1 (Critical issues)
   for i in 1 2 3; do
     gh issue create --title "Phase 1 issue $i" --body-file /tmp/phase1_issue$i.md --label "critical,enhancement" --assignee @me
   done

   # Phase 2 (Important issues)
   for i in 1 2 3 4; do
     gh issue create --title "Phase 2 issue $i" --body-file /tmp/phase2_issue$i.md --label "enhancement,refactoring" --assignee @me
   done

   # Phase 3 (Nice-to-have issues)
   for i in 1 2 3 4 5; do
     gh issue create --title "Phase 3 issue $i" --body-file /tmp/phase3_issue$i.md --label "refactoring" --assignee @me
   done

3. Verify by phase (using grep to filter):
   gh issue list --state open --limit 20 --json number,title,assignees | grep -E "Phase 1|Phase 2|Phase 3"
```

This approach ensures:

- All body files ready before creating issues (easier to review)
- Clear separation of phases
- Easy verification of which issues were created
- Simple recovery if one phase fails

### Single-issue fast path (still using body files)

```sh
# 1) Create body file with your IDE tools, e.g. /tmp/issue.md

# 2) Create issue
gh issue create --title "Fix save state corruption" --body-file /tmp/issue.md --label bug --assignee @me
```

### Update existing issue

```sh
gh issue comment 123 --body "Progress: tests added; implementation in progress."
```

## Best Practices

1. **Run preflight checks first**:
   - Confirm account: `gh api user --jq .login`
   - Confirm labels: `gh label list --json name --limit 100`
   - Optional repo check: `gh repo view --json nameWithOwner`
   - This prevents avoidable failures before writing or submitting issue content

2. **Batch issue creation**: When creating multiple related issues (e.g., Phase 1, 2, 3 recommendations):
   - Create all body files first using IDE file creation
   - Then create all issues in sequence
   - Finally, verify all issues were created with `gh issue list`

3. **Issue body format**:
   - Use clear section headers: `## Problem`, `## Solution`, `## Acceptance Criteria`
   - Keep paragraphs concise and scannable
   - Use bullet points for lists
   - Include relevant links to documentation or code sections
   - Include example code blocks where helpful for understanding the change

4. **Labeling consistency**:
   - Use standard labels: `bug`, `enhancement`, `testing`, `refactoring`, `mapper`, `games`
   - Combine multiple labels for multi-aspect issues: `--label "testing,enhancement"`
   - Document label usage in your project README
   - **Verify labels exist** before creating issues - if label doesn't exist, `gh` will fail with "could not add label: 'X' not found"
   - If a label error occurs, either use a different label or create it in GitHub UI first

5. **Cross-issue references**:
   - When creating sub-issues, document the parent issue clearly in the body
   - Use GitHub's issue linking: "Related to #123" appears as a link
   - Update parent issue with progress when sub-issues are completed

6. **Verify after creation**:
   - Always list recent issues after batch creation to confirm success
   - Check issue formatting with `gh issue view <number>`
   - Fix any formatting issues immediately with `gh issue edit`
   - For filtering verification output, use grep with issue number ranges:
     ```sh
     gh issue list --state open --limit 15 --json number,title | grep -E "530|531|532"
     ```

7. **Organizing related issues as phases or groups**:
   - When creating multiple related issues as part of a larger initiative (e.g., code review recommendations), organize them logically
   - Document the group/phase structure in issue bodies: "Phase 1 (Critical)", "Phase 2 (Important)", "Phase 3 (Nice to Have)"
   - Include cross-references between phases in issue descriptions
   - Create a summary issue or document linking all related issues if group is large (>5 issues)
   - Use consistent naming patterns in titles to make related issues easy to find
   - Consider creating phases sequentially so issue numbers stay grouped
   - Example structure:
     ```
     Phase 1 (Critical): Issues #523-#525 (Correctness fixes)
     Phase 2 (Important): Issues #526-#529 (Consistency improvements)
     Phase 3 (Refactoring): Issues #531-#535 (Design improvements)
     Phase 4 (Documentation): Issues #536-#539 (Documentation polish)
     ```

8. **Complete issue → branch → PR → merge workflow**:

   This is the end-to-end workflow for working on an issue:

   ```
   Step 1: Create the issue (if not already created)
   gh issue create --title "Issue title" --body-file /tmp/issue.md --label bug --assignee @me
   # Returns issue number (e.g., #492)

   Step 2: Create a branch from main
   git checkout main
   git pull origin main
   git checkout -b 492-short-description
   # Branch name format: <issue-number>-<short-kebab-case-description>

   Step 3: Implement, test, and commit
   # Make changes
   cargo test --all-features  # Verify tests pass
   cargo fmt
   cargo clippy --all-targets --all-features -- -D warnings
   git add <files>
   git commit -m "Fixes #492: description of the change"

   Step 4: Push the branch to remote
   git push origin 492-short-description

   Step 5: Create the PR
   gh pr create \
     --title "Fix #492: Implement SOCD handling for joypad" \
     --body "Description of what was changed and why.

   ## Changes
   - What was changed

   Fixes #492" \
     --head 492-short-description
   # The --head flag ensures gh knows which branch to use, avoiding interactive prompts

   Step 6: Request review
   # Share the PR URL with your navigator/reviewer
   # Wait for approval and feedback

   Step 7: Merge the PR (after approval and all checks pass)
   gh pr merge <pr-number> --squash --delete-branch
   # --squash: Combines all commits into one for clean history
   # --delete-branch: Automatically deletes the feature branch after merge

   Step 8: Verify and close related issue
   # The issue is auto-closed if PR body contains "Fixes #492"
   # Verify it's closed: gh issue view 492
   ```

9. **Complete multi-phase workflow**:

   When working from a comprehensive code review or analysis document:

   ```
   Phase A: Analysis & Issue Creation
   - Conduct code review/analysis
   - Document findings in structured markdown (REVIEW.md)
   - Organize recommendations into phases by priority
   - Define acceptance criteria for each issue
   - Create all body files using IDE file creation tools

   Phase B: Batch Create All Issues
   - Create Phase 1 (Critical) issues with --assignee @me
   - Verify successful creation: gh issue list --state open --limit 10
   - Create Phase 2 (Important) issues
   - Create Phase 3 (Refactoring) issues
   - Create Phase 4 (Documentation) issues

   Phase C: Work on Issues (One at a Time)
   For each issue (in priority order):
   - Create branch: git checkout -b <issue-number>-description
   - Implement and test locally
   - Push branch: git push origin <branch-name>
   - Create PR: gh pr create --title "Fix #<number>: ..." --head <branch-name>
   - Request review and wait for approval
   - Merge: gh pr merge <pr-number> --squash --delete-branch
   - Verify issue is closed automatically

   Phase D: Track Overall Progress
   - Update REVIEW.md with links to created issues
   - Monitor issue completion with: gh issue list --assignee @me --state open

   This phased approach ensures:
   - All issues planned and approved before any work starts
   - Work is tracked per issue with clear ownership
   - Each issue has a corresponding PR for review
   - Clean commit history with squashed merges
   - Automatic tracking via issue closure on PR merge
   ```
