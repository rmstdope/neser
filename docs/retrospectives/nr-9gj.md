# nr-9gj — retrospective

- **Implementer:** Wolverine
- **Date:** 2026-09-27
- **PR:** #3252

## A change to `.cerebro/project.conf` is invisible to the fleet scripts run from its own worktree

**What happened.** After declaring `rust_paths` in the worktree's `.cerebro/project.conf`, `.cerebro/cerebro/scripts/project-conf rust_paths` still printed "rust_paths unset", and `build-workload --classify` still exited 3. The new Python test failed on 22 subtests even though the edit was in place.

**Why.** `project-conf` reads `<shared root>/.cerebro/project.conf`, and in a fleet worktree the shared root is the main checkout (`consumer-root --shared` resolves through `--git-common-dir`). The launcher also exports `CEREBRO_CONSUMER_ROOT` / `CEREBRO_CONSUMER_SHARED_ROOT` pointing at the main checkout. So until merge, every fleet script reads main's declaration, whatever the branch says.

**Cost.** About ten minutes reading `project-conf`, `consumer-root` and `root-hints.sh`. The test and the validation both had to point the root hints at the checkout (`CEREBRO_CONSUMER_ROOT`, `CEREBRO_CONSUMER_SHARED_ROOT` = the tree, `CEREBRO_CONSUMER_MOUNT=.cerebro/cerebro`) to exercise the change.

**Prevent by.** A line in `.cerebro/cerebro/skills/produce-bead/SKILL.md` under *Building*, beside "A changed shared-root declaration is gated in a clone": until merge, the fleet scripts in a worktree read the main checkout's `project.conf`. To validate a changed key there, export the three `CEREBRO_CONSUMER_*` hints pointing at the tree, as `scripts/test_project_conf_rust_paths.py` does.

**Seen before.** None found.
