# nr-kds: one headless run writes a capture every K frames

## What surprised the session

**A fresh worktree fails 334 SNES integration tests until its submodules are checked out.**
`git worktree add` creates the gitlink for `roms/snes/automated_tests/snes_test_roms` (and
`vendor/slang-shaders`) but leaves the directory empty, so the first `./scripts/gate-full.sh`
in the worktree stopped at `cargo test --no-default-features --lib` with 334 failures, every one
`failed to read ROM roms/snes/automated_tests/snes_test_roms/...: No such file or directory`.
The tail of the gate output shows only test names, so it read like a regression until one test
was rerun alone and printed its message. `git submodule update --init --recursive` in the
worktree fixed it; `.cerebro/cerebro/scripts/prepare-worktree` does this for fleet sessions, and a
session that adds a worktree by hand must do the same.

**Mesen2 renders DSP games black without firmware.** In the SNES game sweep this bead was built
for, Super Mario Kart (DSP-1B), SD Gundam GX (DSP-3) and Top Gear 3000 (DSP-4) came out black
in Mesen2 at every checkpoint while NESER showed their title screens. Mesen2's `Firmware`
folder was empty; its stdout log names the coprocessor (`Coprocessor: DSP1B`) but prints no
warning about the missing firmware in `--testRunner` mode. Those three were excluded from the
comparison rather than filed as NESER bugs.

**Both emulators load battery saves by ROM file name.** NESER reads the `.sav` next to the ROM
and Mesen2 its own `Saves/<name>.srm`, so a sweep over a ROM folder that has been played in
either emulator does not start from blank SRAM. Copying the ROMs under a fresh prefix gave both
sides a clean start without touching the person's saves.

## What changed because of it

`--capture-every` itself: the sweep had been taking one NESER run per checkpoint, nearly seven
minutes per game for twelve checkpoints, and the navigator asked for the series capture to be a
standard feature rather than a scratch patch. The README's headless section documents it.
