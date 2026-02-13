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
   - For `--body`, **always use `--body-file`** instead of inline content.
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

10. **Troubleshooting - Label not found**:

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
   gh issue create --title "Issue 1 title" --body-file /tmp/issue1.md --label enhancement
   gh issue create --title "Issue 2 title" --body-file /tmp/issue2.md --label refactoring
   gh issue create --title "Issue 3 title" --body-file /tmp/issue3.md --label testing

3. Verify:
   gh issue list --state open --limit 5 --json number,title
```

**Advantages:** No shell quoting issues, supports unlimited issue body length, clear separation of concerns.

### Batch creation for large grouped initiatives

When creating many related issues (e.g. Phase 1, 2, 3 recommendations from code review):

```
1. Create all body files first (e.g., phase1_issue1.md through phase3_issue5.md)

2. Create issues in priority order:

   # Phase 1 (Critical issues)
   for i in 1 2 3; do
     gh issue create --title "Phase 1 issue $i" --body-file /tmp/phase1_issue$i.md --label "critical,enhancement"
   done

   # Phase 2 (Important issues)
   for i in 1 2 3 4; do
     gh issue create --title "Phase 2 issue $i" --body-file /tmp/phase2_issue$i.md --label "enhancement,refactoring"
   done

   # Phase 3 (Nice-to-have issues)
   for i in 1 2 3 4 5; do
     gh issue create --title "Phase 3 issue $i" --body-file /tmp/phase3_issue$i.md --label "refactoring"
   done

3. Verify by phase (using grep to filter):
   gh issue list --state open --limit 20 --json number,title | grep -E "Phase 1|Phase 2|Phase 3"
```

This approach ensures:

- All body files ready before creating issues (easier to review)
- Clear separation of phases
- Easy verification of which issues were created
- Simple recovery if one phase fails

### Alternative: Creating issues with inline short bodies

```sh
gh issue create --title "Fix save state corruption" --body "Problem: ..." --label bug
```

### Update existing issue

```sh
gh issue comment 123 --body "Progress: tests added; implementation in progress."
```

## Best Practices

1. **Batch issue creation**: When creating multiple related issues (e.g., Phase 1, 2, 3 recommendations):
   - Create all body files first using IDE file creation
   - Then create all issues in sequence
   - Finally, verify all issues were created with `gh issue list`

2. **Issue body format**:
   - Use clear section headers: `## Problem`, `## Solution`, `## Acceptance Criteria`
   - Keep paragraphs concise and scannable
   - Use bullet points for lists
   - Include relevant links to documentation or code sections
   - Include example code blocks where helpful for understanding the change

3. **Labeling consistency**:
   - Use standard labels: `bug`, `enhancement`, `testing`, `refactoring`, `mapper`, `games`
   - Combine multiple labels for multi-aspect issues: `--label "testing,enhancement"`
   - Document label usage in your project README
   - **Verify labels exist** before creating issues - if label doesn't exist, `gh` will fail with "could not add label: 'X' not found"
   - If a label error occurs, either use a different label or create it in GitHub UI first

4. **Cross-issue references**:
   - When creating sub-issues, document the parent issue clearly in the body
   - Use GitHub's issue linking: "Related to #123" appears as a link
   - Update parent issue with progress when sub-issues are completed

5. **Verify after creation**:
   - Always list recent issues after batch creation to confirm success
   - Check issue formatting with `gh issue view <number>`
   - Fix any formatting issues immediately with `gh issue edit`
   - For filtering verification output, use grep with issue number ranges:
     ```sh
     gh issue list --state open --limit 15 --json number,title | grep -E "530|531|532"
     ```

6. **Organizing related issues as phases or groups**:
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

7. **Complete workflow for multi-phase initiatives**:
   
   When working from a comprehensive code review or analysis document:
   
   ```
   Step 1: Analysis & Prioritization
   - Conduct code review/analysis
   - Document findings in structured markdown (REVIEW.md)
   - Organize recommendations into phases by priority
   - Define acceptance criteria for each recommendation
   
   Step 2: Create Issue Bodies
   - Create body files for each phase using IDE file creation
   - Name files consistently: phase1_issue1.md, phase1_issue2.md, etc.
   - Include clear sections: Problem, Current State, Solution, Acceptance Criteria, Reference
   - Review all body files before creating issues
   
   Step 3: Create Issues by Phase
   - Create Phase 1 (Critical) issues first
   - Verify they were created successfully
   - Create Phase 2 (Important) issues
   - Verify success
   - Continue for subsequent phases
   
   Step 4: Verify Complete Initiative
   - List all created issues with filtering
   - Review issue formatting with gh issue view
   - Update REVIEW.md with issue links
   - Create summary comment linking all phases
   
   Step 5: Track Progress
   - Mark phases complete as work progresses
   - Update issue bodies with implementation notes
   - Close issues as PRs merge
   ```
