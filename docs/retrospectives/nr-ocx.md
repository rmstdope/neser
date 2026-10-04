# nr-ocx — retrospective

- **Implementer:** Storm
- **Date:** 2026-10-04
- **PR:** #3335

## A rebase brought in a test that imported constants this PR had moved, and only CI saw it

**What happened.** This PR moved the Mesen2 flag lists `NES_FLAGS`/`SNES_FLAGS` out of
`scripts/test_mesen2_capture.py` into `scripts/reference_capture/compare_mesen2.py`. While it was
in review, nr-ggx (#3333) merged `scripts/test_mesen2_traces.py`, which imports those two names
from `test_mesen2_capture`. The rebase applied without a conflict in any Python file. After it I
ran only the two Mesen2 test modules, ruff and mypy, all green. CI's `python-tests` then failed
with `ImportError: cannot import name 'NES_FLAGS' from 'scripts.test_mesen2_capture'`.

**Why.** A semantic conflict: textually separate files, so git had nothing to merge. mypy did not
catch it either, because `scripts/pyproject.toml` sets `ignore_errors = true` for every module
except a short list, and the test modules are not on it.

**Cost.** One CI round (about five minutes) and one of the three fix attempts.

**Prevent by.** After a rebase onto a main that changed anything under `scripts/`, rerun the full
`python -m unittest discover -s scripts -t . -p "test_*.py"` (about a minute) before pushing,
not only the modules the PR touches. A step for that belongs in `produce-bead`'s *Merging*,
where a `422` conflict sends the producer to rebase.

**Seen before.** none found
