# nr-5ex — retrospective

- **Implementer:** Nightcrawler
- **Date:** 2026-10-24
- **PR:** #3386

## compare_mesen2 reports "capture failed" for every checkpoint past about frame 2400

**What happened.** `python -m scripts.reference_capture.compare_mesen2 <rom> --frames 300 600 … 3600` printed `frame 2100: capture failed: Mesen2 saved no screenshot` (or the same at 2400 or 2700) and stopped, for all five ROMs. This happened both with five comparisons running in parallel and with one running alone while a full gate built in the background. Adding `--mesen2-arg=--timeout=110` made every frame capture.
**Why.** The script hard-codes `--timeout=30` in `COMMON_MESEN2_FLAGS`. That is Mesen2's test-runner wall-clock limit, and running 2400+ frames with frame skipping off takes longer than 30 s on a loaded machine. The script's own subprocess timeout is 120 s, so Mesen2 gives up first and saves no screenshot.
**Cost.** About 15 minutes of reruns, and one wrong first impression that the comparisons were incomplete.
**Prevent by.** In `scripts/reference_capture/compare_mesen2.py`, make Mesen2's `--timeout` scale with the frame count, or default it close to the 120 s subprocess limit. Until then, `scripts/reference_capture/README.md` should say that frames past about 2000 need `--mesen2-arg=--timeout=110`.
**Seen before.** none found.

## A review-answer commit never reached the PR, and main moved into a conflict meanwhile

**What happened.** The branch was pushed once as `HEAD:refs/heads/nr-5ex-vblank-bus-address` from a local branch named `nr-5ex`. After answering the review, `git push -q` printed nothing and pushed nothing. The CI wait then polled a PR head that never changed until the loop timed out. By then nr-6wgi had merged and the PR was `CONFLICTING`.
**Why.** The local and remote branch names differ, and `push.default=simple` refuses that silently under `-q`. An earlier session had pushed under the longer name, and the worktree branch kept the short one.
**Cost.** A 10-minute wasted CI wait, and a conflict rebase with a re-gate that would otherwise have been a plain `update-branch`.
**Prevent by.** The *Waiting* poll in `produce-bead` already compares the PR head with the local `HEAD`. Run that comparison right after every push, before waiting, so a push that went nowhere fails at once instead of after the loop's timeout.
**Seen before.** none found.
