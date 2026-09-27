# nr-zdy.2 — retrospective

- **Implementer:** Storm
- **Date:** 2026-09-27
- **PR:** #3230

## Three sibling beads changed the same F8 code at the same time, and this one rebased twice

**What happened.** nr-zdy.1 (web Palette button), nr-zdy.2 (this, F8 for Game Boy Color colour correction) and nr-zdy.3 (F8 and the Colors button for GBA colour correction) were produced in parallel. All three edit the same places: the `KeyOutcome::CyclePalette` dispatch in `src/frontends/native/event_loop.rs`, `cyclePaletteAction` and the Colors-button handler in `web/src/app.ts`, `web/src/display/cgb_color_correction.ts`, the `WasmGb` type declarations and the README F8 row. After review, nr-zdy.3 merged and the PR went `CONFLICTING`. After a hand resolution and a full gate, nr-zdy.1 had merged and it conflicted again.
**Why.** The epic nr-zdy was split into one child per console, with no dependency edges between the children. The fleet therefore offered all of them at once, although each child's plan names the same shared F8 and Colors-button code.
**Cost.** About an hour: two rebases, two extra full-gate runs (~20 min each on a machine running other gates), and one extra review of the resolution.
**Prevent by.** When an epic's children share one dispatch point, chain them with `bd dep add` at split time (`write-bead` / Cerebro's splitting step), so `assignable-beads` offers them one after another. Alternatively, a producer's design step could check open `in_progress` siblings whose plans name the same files.
**Seen before.** nr-72o (three sibling DSP beads claimed in the same minute, 14-file rebase).

## Known local traps, again

**What happened.** wasm-pack's ChromeDriver 154 against Chrome 153 (`http status: 404`), and no `.venv` in the prepared worktree. The recorded fixes worked: a 153 driver first on `PATH`, and a worktree `.venv` from `scripts/pyproject.toml`'s `test` and `dev` groups.
**Why.** As nr-273 and nr-qoi recorded.
**Cost.** About ten minutes.
**Prevent by.** As nr-273 and nr-qoi.
**Seen before.** nr-273, nr-qoi, nr-zdy.3 and others.
