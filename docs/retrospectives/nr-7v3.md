# nr-7v3 retrospective

## A per-game "fresh" ROM name still leaks Mesen2's battery save between runs of that game

**What happened.** The bead's rerun recipe copies the ROM once, to
`out/fresh-super-mario-rpg-legend-of-the-seven-stars.sfc`, and then runs Mesen2 twelve times on
that one file. Mesen2 writes `Saves/<name>.srm` when a run exits, so every later run under that
name boots on the SRAM the earlier run left behind. My twelve checkpoint captures happened to
start together and all ran from blank SRAM. The state dumps and per-frame captures I took
afterwards under the same name did not. Frame 600 of a later run differed by 31% from frame 600
of the first run of the same build. For a while that read as an emulator divergence.

**Why.** The name is fresh per game, not per run. nr-nuf recorded this trap for a name shared
between games, and its prevention (a name unique per ROM *and* per run) has not reached
`scripts/reference_capture/README.md` or the rerun recipes the sweep beads carry. This bead's
recipe has the same shape.

**Cost.** About twenty minutes: a contaminated register/CGRAM comparison, a frame-by-frame
diff against the wrong reference, then finding the `.srm` in
`~/Library/Application Support/Mesen2/Saves/`. After that, every Mesen2 run went through a
wrapper that copies the ROM to a unique name per run and deletes the `.srm` afterwards.

**Prevent by.** The Mesen2 recipe in `scripts/reference_capture/README.md`, and the rerun block
written into every game-sweep bead: copy the ROM to a new name for **each Mesen2 invocation**
(for example `fresh-<rom>-<pid>-<nanoseconds>.sfc`) and remove `Saves/<that name>.srm` after
the run, rather than one copy reused across a checkpoint loop.

**Seen before.** `docs/retrospectives/nr-nuf.md` (a constant name shared between games) and
`docs/retrospectives/nr-kds.md` (both emulators load battery saves by ROM name).
