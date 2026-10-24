# nr-ayk5.1 — the producer's launch prompt named no bead, though one was claimed for it

## What happened
The producer session (Cyclops) was started with a prompt that named no bead. `produce-bead` step 1 says "with no bead in the prompt, run `end-pass` and stop". Yet nr-ayk5.1 was `in_progress`, assigned to Cyclops, had a prepared worktree, and held seven commits from an earlier session that had died before pushing or opening a PR. The producer found the bead with `bd list --status in_progress --assignee Cyclops` and carried on, instead of ending the pass.

## Why
Not established. Either the fleet view relaunched the role after the earlier session died without putting the claimed bead into the new prompt, or the prompt template has no bead slot on a restart.

## Cost
A few minutes of judgement and lookup. Following the rule literally would have stranded the claim and the unpushed commits until somebody noticed.

## Prevent by
Two options: the relaunch path in `.cerebro/cerebro/scripts/launch` (or the fleet view's restart) puts the bead the session already holds into the prompt, or `produce-bead` step 1 checks `bd list --status in_progress --assignee <name>` before ending a pass that has no bead.

## Seen before
No.
