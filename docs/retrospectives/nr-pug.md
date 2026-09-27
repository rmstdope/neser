# nr-pug — retrospective

- **Implementer:** Storm
- **Date:** 2026-09-27
- **PR:** #3263

## The web TypeScript is type-checked by nothing, and main already carries 12 type errors

**What happened.** This change altered the wasm bindings' TypeScript surface (`cycle_palette`'s meaning, the new `color_label`), so I ran `npx tsc --noEmit -p tsconfig.json` to check `web/src/app.ts` against it. It printed 12 errors that already exist on main: 5 in `app.ts` (for example `is_original_game` and `set_original_games_on_color` missing from `WasmGb`, and `WasmGb` not assignable to `SaveStateRuntime`), 1 in `touch_controls.ts`, and 6 in tests. `web/types/neser-wasm.d.ts` is what types `../pkg/neser`, because the built `web/pkg` ships no `.d.ts`. It had also never declared the `WasmGba` colour-correction methods that `app.ts` called. Neither `scripts/gate-full.sh` nor CI runs `tsc` (Vite and Vitest strip types without checking them), so every one of these went unnoticed.
**Why.** No gate step type-checks `web/`, and the hand-kept `neser-wasm.d.ts` drifts from the Rust bindings with nothing to compare it against.
**Cost.** About ten minutes telling the existing errors apart from this change's. Nothing was wrong in this PR, but a real typing mistake here would have landed just as silently.
**Prevent by.** A `tsc --noEmit -p tsconfig.json` step in `scripts/gate-full.sh` and CI, after the 12 existing errors are fixed. Or have wasm-pack emit its `.d.ts` into `web/pkg` so the bindings type themselves and `web/types/neser-wasm.d.ts` can go.
**Seen before.** None found.
