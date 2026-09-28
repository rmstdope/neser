# nr-2wn retrospective

## A golden named `…_matches_mesen2` was 238 px off Mesen2, and only a fix that moved it showed that

**What happened.** The fix, which applies INIDISP per pixel, moved the CRC of
`inidisp_enable_display_mid_frame_matches_mesen2`. A fresh Mesen2 capture of the new frame
showed 45 px of difference, which looked like a regression. The same capture of the old
build then showed 238 px: the golden had been NESER's own frame for some time. Its comment
said "matching Mesen2 exactly", and the module header said every golden in the file is a
0-pixel match.

**Why.** Not established. The golden dates from 2026-07-07 (92d6fa1a). Some later timing or
rendering change on either side probably moved the reference without moving NESER's CRC.
Nothing re-checks a passing screen-CRC golden against the reference, so the claim in the
comment went stale while the test stayed green.

**Cost.** About 15 minutes: a second release build of the pre-change tree, and an extra
Mesen2 capture to be sure the fix improved this ROM rather than regressed it. Without the
old-build capture, the 45 px would have read as a regression, and the skill's rule "a fix
that improves several vectors and destroys one: do not ship it" would have blocked a
correct fix.

**Prevent by.** In `.github/skills/snes-hardware-research/SKILL.md`, *Measure the pre-change
baseline BEFORE touching golden-shifting code*: when a golden moves, diff **both** the
pre-change and post-change frames against a fresh Mesen2 capture before calling the move a
regression. A green `…_matches_mesen2` test proves the frame is unchanged, not that it
still matches Mesen2.

**Seen before.** The same class as #3092 (`#[ignore]`d goldens that rotted, one of them
asserting another ROM's CRC), but here the test was not ignored and ran green on every PR.
