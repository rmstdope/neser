# nr-kis: retrospective

- **Implementer:** Nightcrawler
- **Date:** 2026-09-29
- **PR:** #3320

## A follow-up push went nowhere, and nothing said so

**What happened.** The prepared tree's branch is `nr-kis`. I opened the PR from
`git push -u origin HEAD:nr-kis-top-gear-vofs-fetch` to follow the `<id>-short-description`
convention. After the review fixes, a plain `git push -q ... | tail -1` printed nothing, and the
PR stayed on the old head. With `push.default=simple`, git refuses to push when the upstream
branch name differs from the local one, and `-q` plus `tail` hid the refusal. I only noticed
because the mergeability poll showed the old `headRefOid`.
**Why.** The local branch name and the remote branch name differed, and I filtered the push's
output.
**Cost.** A few minutes, plus a mergeability poll that would have waited on the wrong head.
**Prevent by.** Push follow-ups with an explicit refspec (`git push origin HEAD:<remote-branch>`),
or open the PR from the tree's own branch name. After every push, compare
`gh pr view <n> --json headRefOid` with `git rev-parse HEAD` before waiting on CI. The
`produce-bead` *Merging* poll already reads the head: check that it is yours.
**Seen before.** None found (`grep -rl "HEAD:" docs/retrospectives/` is empty).

## Reverting a mutation with `git checkout <file>` threw away an uncommitted test

**What happened.** To show that a new test catches a dropped fine-scroll term, I mutated
`fetched_bg_vofs` with `sed` and then reverted it with `git checkout src/snes/ppu/background.rs`.
That also discarded the new, uncommitted test in the same file, and I had to write it again.
**Why.** The test and the mutation were uncommitted changes to the same file.
**Cost.** One rewrite of a 50-line test.
**Prevent by.** Commit the test before running a mutation, then revert the mutation with
`git checkout` (now safe) or with the inverse `sed`. The `snes-hardware-research` step "Verify
the golden's power by mutation" is where this belongs.
**Seen before.** None found.
