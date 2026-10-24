# nr-i5pl

## The prepared worktree's shader submodule was at the wrong commit, half deleted

**What happened.** The prepared tree arrived with `git status` showing `M vendor/slang-shaders`. The submodule's HEAD was `2b0530b` ("Update geom.slang #976"), not the commit `cb8e534` that main records, and its index held staged deletions of files including `handheld/gameboy.slangp`. The main checkout's submodule was correct. The first full gate stopped at `cargo test --all-features --lib`, where `shader_manager::tests::gb_lcd_preset_colour_palette_texture_is_replaced` panicked with `IOError("vendor/slang-shaders/handheld/gameboy.slangp", NotFound)`. That reads like a regression in a web-only diff. `git submodule update --init --force vendor/slang-shaders` followed by `git -C vendor/slang-shaders reset --hard HEAD` fixed it, and the next gate passed.

**Why.** Not established. The tree was made by `scripts/prepare-worktree` before this session started. Something moved the submodule's HEAD and staged deletions after its `git submodule update --init`, or the update ran against a module git dir another tree had left at a different commit.

**Cost.** One full gate run, about 25 minutes, plus diagnosis.

**Prevent by.** Have `.cerebro/cerebro/scripts/prepare-worktree` finish with `git submodule status --recursive` in the new tree, and refuse to hand it over if any line starts with `+` or `-`, or if `git -C <submodule> status --porcelain` is non-empty. The first producer step in `produce-bead` already reads `git status --porcelain`. It could say that a modified submodule is a preparation fault to fix with `git submodule update --init --force` before building.

**Seen before.** `docs/retrospectives/nr-kds.md` covers an *empty* submodule in a hand-made worktree. This one was initialised but at the wrong commit, in a fleet-prepared tree. The missing `.venv` in this tree, which `./scripts/setup-venv.sh` fixed, is already recorded in `docs/retrospectives/nr-qoi.md`.
