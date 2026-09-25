# nr-aph — retrospective

- **Implementer:** Bishop
- **Date:** 2026-09-25
- **PR:** #3194

## The Mesen2 capture recipe's "0-px match" never compared a lit colour

**What happened.** After the palette-RAM fix, NESER's frame-120 capture of `power_up_palette.nes` still differed from Mesen2's by 69 px. Every differing pixel was white text: NESER's `default` system palette gives #ECEEEC and Mesen2's gives #FFFEFF. `scripts/reference_capture/README.md` and the `nes-hardware-research` skill cite a verified 0-px NES match on `instr_test-v5/official_only.nes` at frame 120, but NESER's capture of that frame is 61440 pixels of (0,0,0).
**Why.** The recipe was checked on a frame with no lit content, so a system-palette mismatch could not show. The bead was filed from that belief: it expected 0 px and misread the pre-fix diff as "prints nothing" (Mesen2 prints `$01`).
**Cost.** One question to the navigator, one follow-up bead (nr-pzt), and a bead that ships short of its literal "pixel-for-pixel" acceptance.
**Prevent by.** The recipe's NES verification in `scripts/reference_capture/README.md` should use a frame with lit, non-black pixels. Its claim should be corrected until the palette question (nr-pzt) is settled.
**Seen before.** None found.

## wasm-pack test failed locally with "http status: 404" from ChromeDriver, a fourth time

**What happened.** The `wasm-pack test --headless --chrome` gate leg failed before any test ran. `CHROMEDRIVER=<path>` is overridden by wasm-pack; only `--chromedriver <path>` works.
**Why.** As before: the cached ChromeDriver is 154.0.8037.57 and the installed Chrome is 153.0.8010.53.
**Cost.** About fifteen minutes. The leg passed (94 tests) with ChromeDriver 153.0.8010.52 from Chrome for Testing.
**Prevent by.** See nr-09s; still unfixed on this machine.
**Seen before.** nr-09s, nr-qoi, nr-6e9, nr-nr7.

## The worktree has no Python venv, so the gate's Python legs cannot run

**What happened.** `scripts/gate-full.sh` uses `.venv/bin/python` when it exists and otherwise `python3`. The prepared worktree has no `.venv`, and the system `python3` has no ruff or mypy. The main checkout's `.venv` lacks mypy, and 28 unittest modules fail to import there (textual, requests and others).
**Why.** `.cerebro/project.conf` declares only `npm ci` as `install`; nothing installs the `scripts/pyproject.toml` dependency groups.
**Cost.** About five minutes: a venv built in the scratchpad (`pip install --group scripts/pyproject.toml:test --group scripts/pyproject.toml:dev`), after which all 452 tests, ruff and mypy passed.
**Prevent by.** Add the dependency-group install to the project's `install` declaration, and gitignore `.venv` in the tree.
**Seen before.** None found.
