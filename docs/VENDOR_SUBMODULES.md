# Vendored Submodule Refresh Policy

This document defines when and how NESER's vendored Git submodules are updated, and how to
re-verify the repository afterwards.

NESER vendors two submodules (see `.gitmodules`):

| Path | Upstream | Used by |
| ---- | -------- | ------- |
| `vendor/slang-shaders` | [libretro/slang-shaders](https://github.com/libretro/slang-shaders) | Native-frontend shader presets |
| `roms/snes/automated_tests/snes_test_roms` | [rmstdope/snes-test-roms](https://github.com/rmstdope/snes-test-roms) | SNES automated tests |

## Why they are pinned

The pin is the point. Release archives ship only the shader files reachable from the presets
configured in `src/platform/shaders.rs`, so the pinned commit determines exactly what a release
contains and how those presets look. Nothing in the build wants a newer upstream, and no test
checks that a preset still *renders* correctly, so moving the pin changes user-visible output
with no automatic signal.

## Cadence: on demand, never scheduled

There is no periodic refresh. Bump a submodule only when there is a specific reason:

- a new preset is being added to `SHADER_PRESETS`, and it needs a file the current pin lacks;
- a specific upstream fix is needed;
- for `snes_test_roms`, new test assets have been merged upstream.

**An unexplained pin move in an unrelated PR is a defect. Revert it rather than rationalising
it.** This is not hypothetical — it is the failure mode that has actually occurred here.
`vendor/slang-shaders` has oscillated between two commits, twice moving as a side effect of an
unrelated SNES change (`3bc2edee`, `884a64ec`), with one deliberate "bump" (`16068cd9`) moving it
*backwards*. `snes_test_roms` had the same problem in `fe38cd50`, restored by `e62d5fc4`.

## Bumping deliberately

```bash
git -C vendor/slang-shaders fetch origin
git -C vendor/slang-shaders checkout <upstream-sha>
git add vendor/slang-shaders
```

Stage the gitlink on its own, in a commit that does nothing else, and say in the commit message
which upstream commit you moved to and why. A pin move that is easy to review is a pin move that
stays correct.

## Avoiding accidental moves

A submodule worktree that is out of date with the pin will be staged as a pin *regression* if you
`git add -A`. Two habits prevent it:

```bash
git submodule update --init --recursive   # after every pull or branch switch
git submodule status                      # before committing — expect no leading +/-
git diff --cached --submodule             # shows any staged gitlink move explicitly
```

A leading `+` in `git submodule status` means the checked-out commit differs from the pin. Resolve
that before committing, in whichever direction is correct.

CI helps too: `.github/workflows/ci.yml` has a `shader_assets` filter whose bare
`vendor/slang-shaders` entry matches a gitlink-only change, so moving the pin runs the
verification below. Without that entry a pin move matches no filter and runs almost no CI at all.

## Verifying after a bump

```bash
python -m unittest discover -s scripts -t scripts -p "test_package_release.py"
```

Two of those tests run against the real repository tree:

- `test_repository_shader_dependencies_are_collectable` walks every `.slangp` reachable from
  `SHADER_PRESETS` and follows its `shader0=`, `#reference` and `#include` edges.
  `_collect_shader_file` raises `FileNotFoundError` on a missing target, so a moved or renamed
  shader file fails here — and, if it ever got past here, would fail the release build.
- `test_repository_shader_preset_paths_match_exact_vendor_casing` checks each configured preset
  path component-by-component with exact case. macOS filesystems are case-insensitive by default,
  so an upstream rename that only changes capitalisation would otherwise work locally and break on
  Linux.

### What this does not prove

- **Textures are not covered.** `_collect_existing_lut` finds textures with a loose regex and then
  filters on `exists()`, which cannot distinguish a renamed texture from a regex false positive —
  so a preset can package and ship without its lookup table, silently. Tracked in
  [#3123](https://github.com/rmstdope/neser/issues/3123).
- **Appearance is not covered.** No test renders through a shader. If a bump changes how a preset
  looks, only running the emulator will show it. Check the presets you care about by hand.
- **Preset paths are duplicated.** `src/platform/shaders.rs` is the source of truth, but the same
  paths appear as string literals in `src/nes/console/config/cli.rs` tests. If an upstream rename
  forces a path change, update both.

## Per-submodule notes

### `vendor/slang-shaders`

Everything above applies. Note the clone is `shallow = true`, so `git -C vendor/slang-shaders log`
shows only recent history; fetch more depth if you need to inspect an older commit.

### `roms/snes/automated_tests/snes_test_roms`

Bump mechanics and the accidental-move guidance above apply unchanged. Provenance, manifest
metadata, licensing and the committed-CI-subset rules are **not** repeated here — they are
specified in [SNES_TEST_ASSET_POLICY.md](SNES_TEST_ASSET_POLICY.md), which is the authority for
that submodule's contents. SNES test commands and the golden-baseline approval workflow are in
[README-SNES.md](../README-SNES.md).
