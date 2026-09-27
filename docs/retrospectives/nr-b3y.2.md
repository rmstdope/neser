# nr-b3y.2 — retrospective

- **Implementer:** Rogue
- **Date:** 2026-09-27
- **PR:** #3277

## Conflict markers were committed during a scripted rebase resolution

**What happened.** While rebasing onto main after nr-upb (#3272), the `architecture.md` conflict was resolved by a one-off Python script. That script ran in the same shell command as `git add architecture.md && GIT_EDITOR=true git rebase --continue`, joined with `;`, not `&&`. The script raised `ValueError: too many values to unpack` and changed nothing, but `git add` and `rebase --continue` ran anyway. The docs commit `811ac74f` went in with `<<<<<<<`/`>>>>>>>` markers. It was noticed only because the next conflict stop printed the file still unresolved. The fix went into the next commit, so the branch head was clean.

**Why.** The resolve step and the commit step were chained with `;`, and nothing checked for markers before `git add`.

**Cost.** About ten minutes. One intermediate commit on the branch carries markers; the squash-merge hides it, but a non-squash merge would have put it on main.

**Prevent by.** In `produce-bead`, *Merging* (the `422` / `CONFLICTING DIRTY` rebase path), add a step before each `git rebase --continue`: `git diff --cached --check` (or `grep -rn '^<<<<<<<\|^>>>>>>>' <files>`) must come back clean. Run the continue as a separate command from any scripted resolution, never chained after it with `;`.

**Seen before.** None found.

## Rebase conflicts could not be resolved by taking one side of a file

**What happened.** During the same rebase, `git checkout --ours <files>` was refused by the auto-mode permission classifier as "Irreversible Local Destruction". The plan had been to take main's side of the conflicted files and re-apply the bead's change on top. Every hunk was resolved by hand instead.

**Why.** The classifier treats discarding one side of a conflict as destroying work, even when that side is safe in a pushed commit.

**Cost.** Roughly 20 minutes of hunk-by-hunk resolution across four files, and it opened the way for the scripted slip above.

**Prevent by.** In `produce-bead`, *Merging*, say that a producer resolves rebase conflicts hunk by hunk, never with `git checkout --ours/--theirs`, which the permission layer refuses. For a large redesign across a moved-under-you file, plan a fresh commit on the new base over a rebase.

**Seen before.** None found.
