# NESER Codebase Improvement Plan

> A prioritized, issue-ready catalogue of refactoring and quality opportunities for the
> NESER multi-system emulator (NES / GB·CGB / GBA / SNES).

## 1. Overview

NESER is a mature, well-tested emulator: ~382,000 lines of Rust across 659 files, four
console cores, three frontends, and ~11,800 unit tests. This document is a **standing
backlog of improvements** — it does not change any code. Each entry is written to be
**issue-ready**: a maintainer can lift an entry into a GitHub issue and begin TDD work
immediately.

**How to read this document**

- Section 2 is the executive summary plus the **impact/effort matrix**.
- Section 3 gives a **dependency-aware recommended sequence** (waves 0–3).
- Section 4 is the **improvement catalogue**, grouped into six areas. Every entry has:
  *Problem · Proposed approach · Affected files · Acceptance criteria · Suggested
  sub-tasks · Dependencies · Suggested labels*.
- Section 5 lists **non-goals** (things deliberately *not* recommended).
- Section 6 is a **metrics appendix** for traceability.

**Conventions**

- **Impact** and **Effort** are rated `Low` / `Medium` / `High`. No time estimates.
- "Affected files" are starting points, not exhaustive change sets.
- Citations use `path:line` where a concrete anchor strengthens the case.
- Improvement IDs (`I1.1`, `I2.3`, …) are stable handles for cross-references and future
  issue titles.

**Scope:** comprehensive — all six areas. **Disposition:** documentation only; GitHub
issues are a deliberate later step.

---

## 2. Executive Summary

The codebase is healthy (clean TODO hygiene, large safety net of tests, clear per-console
layering). The improvement opportunities cluster into six areas:

1. **File / module decomposition** — a handful of impl files have grown into god objects
   (e.g. `rom_browser/app.rs` 3,790 lines; NES `config.rs` 5,575 lines; `keyboard.rs`
   1,998 lines). *(Inline tests are kept — see Non-goals.)*
2. **Cross-console abstraction** — save-state, the `Emulator` trait impls, the headless
   integration-test harness, and debugging primitives are re-implemented per console with
   duplicated logic.
3. **Frontend coupling** — 85 `Console::<Variant>` match sites leak console-specific
   behavior into frontends that should program against the `Emulator` trait.
4. **Build / errors** — a single 382K-line crate (slow incremental builds, weak layering);
   189 stringly-typed `Result<_, String>` boundaries with no typed error model.
5. **Tooling / CI / Python** — CI re-installs tooling each run; Python scripts have no
   linter/formatter/type-check configuration.
6. **Docs hygiene** — `architecture.md` has drifted (claims 183K lines and an `sdl`
   feature; reality is 382K lines and a `native` feature).

### Impact / Effort matrix

| ID | Improvement | Impact | Effort |
|----|-------------|:------:|:------:|
| I2.1 | Shared `Stateful` trait + `platform::save_state` | High | Medium |
| I3.1 | Capability traits to remove `Console::` frontend leakage | High | Med–High |
| I4.2 | Typed errors (`thiserror`) at public/core boundaries | High | Med–High |
| I4.1 | Phased Cargo workspace split | High | High |
| I2.3 | Reduce `Emulator` impl forwarding boilerplate | Medium | Low |
| I2.5 | Consolidate debugging primitives (breakpoints/trace) | Medium | Low |
| I6.1 | Fix `architecture.md` drift | Medium | Low |
| I5.1 | CI caching + integration-suite parallelization | Medium | Low–Med |
| I2.4 | Shared headless integration-test harness | Medium | Medium |
| I2.2 | Save-state format evaluation (JSON→postcard) | Medium | Medium |
| I2.6 | Shared input primitives (serial/shift-register) | Medium | Med–High |
| I3.2 | Deduplicate WASM bindings; add `WasmSnes` | Medium | Low–Med |
| I3.3 | Shared logical-input layer (native ↔ web) | Medium | Medium |
| I1.1 | Decompose `nes/cpu/cpu.rs` impl | Medium | Medium |
| I1.2 | Decompose `rom_browser/app.rs` | Medium | High |
| I1.3 | Split NES `config.rs` | Medium | Medium |
| I1.4 | Split `platform/config.rs` | Medium | Medium |
| I1.5 | Split `keyboard.rs` | Medium | Medium |
| I1.6 | Slim `event_loop.rs` | Medium | Medium |
| I5.2 | Python tooling (ruff/pyproject/type hints) | Medium | Low–Med |
| I4.3 | Audit `dead_code` / `too_many_arguments` allows | Low–Med | Medium |
| I1.7 | Catalogue remaining large impls | Medium | Med–High |
| I6.2 | Consolidate README sprawl | Low–Med | Low |
| I5.3 | Document vendor submodule cadence | Low | Low |
| I6.3 | Add doc-sync check | Low | Low |

**Quadrant reading**

- **High impact / low effort (do first):** I2.3, I2.5, I6.1.
- **High impact / medium effort (anchor work):** I2.1, I4.2, I3.1.
- **High impact / high effort (major initiative):** I4.1.
- **Steady maintenance:** I5.x, I6.2/I6.3, I4.3.

---

## 3. Recommended Sequence (dependency-aware)

In-crate refactors land first; shared abstractions go into `src/platform/`. The workspace
split (I4.1) is a separate, later track so abstractions don't have to wait for crates to
exist.

- **Wave 0 — quick unblockers:** `I6.1` (docs truth), `I2.3` (trait boilerplate),
  `I2.5` (debugging primitives), `I5.1` (CI speed).
- **Wave 1 — shared abstractions in `src/platform/`:** `I2.1` (Stateful trait),
  `I2.4` (test harness), `I3.1` (capability traits), `I4.2` (typed errors).
  *These reduce duplication and de-leak frontends, which eases Wave 2.*
- **Wave 2 — decomposition (eased by Wave 1):** `I1.1`–`I1.6`, `I3.2`, `I3.3`, `I2.6`,
  `I2.2`.
- **Wave 3 — large parallel track:** `I4.1` (start with a leaf `neser-core` crate),
  `I4.3`, `I5.2`, `I1.7`, `I6.2`, `I6.3`.

```
Wave 0 ──> Wave 1 ──> Wave 2 ──> Wave 3
 docs       save-state  file       workspace
 boilerplate harness    splits     split
 debug prims captraits  wasm/input tooling
 CI speed    errors     formats    docs
```

Key dependencies: `I2.2` depends on `I2.1`; `I1.6` couples with `I3.1`; `I3.2`/`I3.3`
benefit from `I3.1`; all Wave-2 file splits are easier once `I4.2` and `I2.1` have
stabilized the affected modules.

---

## 4. Improvement Catalogue

### Area 1 — File / Module Decomposition

> Goal: split oversized **implementation** code by responsibility. Per the agreed
> non-goals, inline `#[cfg(test)]` modules stay where they are (idiomatic Rust); the test
> module simply moves with the impl it covers when a file is split.

#### I1.1 — Decompose `nes/cpu/cpu.rs` impl by responsibility
- **Impact:** Medium · **Effort:** Medium
- **Problem:** `src/nes/cpu/cpu.rs` is 16,847 lines (≈2,486 impl + ≈14,361 inline tests).
  The impl bundles register/state structs, the fetch/decode/execute loop, interrupt
  latching, OAM/DMC DMA stepping, and save-state capture/restore in one `impl Cpu`.
- **Proposed approach:** Within `src/nes/cpu/`, extract cohesive submodules and re-export:
  - `state.rs` — `CpuState` / `CpuRegisters` and save-state capture/restore.
  - `interrupts.rs` — IRQ/NMI latching (`end_cpu_cycle_latch_*`, `service_irq_or_nmi_sequence`).
  - `dma.rs` — OAM/DMC DMA stepping (`handle_oam_dma_if_pending`, `tick_single_dma_cycle`).
  - `execute.rs` — instruction execution; keep `cpu.rs` as the struct + orchestration.
  Use `impl Cpu` blocks across files (Rust allows split inherent impls), or free functions
  taking `&mut Cpu`. Move each test cluster next to the code it exercises.
- **Affected files:** `src/nes/cpu/cpu.rs`, `src/nes/cpu/mod.rs`, new submodules.
- **Acceptance criteria:**
  - No public API change; `cargo test --no-default-features --lib` green with identical test count.
  - `cargo clippy --all-targets --all-features -- -D warnings` clean; `cargo fmt` clean.
  - No single resulting impl file exceeds ~1,500 impl lines.
- **Suggested sub-tasks:** (1) extract `state.rs`; (2) extract `interrupts.rs`;
  (3) extract `dma.rs`; (4) extract `execute.rs`; verify after each.
- **Dependencies:** Lighter after I4.2 (CPU error/`Result` surfaces stabilized).
- **Suggested labels:** `refactoring`

#### I1.2 — Decompose `rom_browser/app.rs` god object
- **Impact:** Medium · **Effort:** High
- **Problem:** `src/frontends/native/rom_browser/app.rs` is 3,790 lines mixing the catalog
  loading state machine, filtering/search/favorites, texture decode/upload/cache, gamepad +
  keyboard navigation, grid/sidebar/overlay rendering, and action routing. Hotspots:
  `set_catalog` (~282), `rebuild_filtered` (~311), `render_frame` (~425),
  `render_grid_egui` (~828), `render_search_panel_egui` (~1238),
  `render_filter_panel_egui` (~1480), `render_detail_view_egui` (~1750),
  `lazy_load_visible_textures` (~2037), `poll_gamepad` (~2449), `apply_action` (~2618).
- **Proposed approach:** Split into focused modules under `rom_browser/`:
  `catalog_state.rs` (load/progress/filter model), `browser_input.rs` (keyboard+gamepad →
  actions), `browser_render.rs` (grid/sidebar/overlays), `texture_cache.rs`,
  `favorites_state.rs`. Keep `RomBrowserApp` as orchestration that owns these and routes
  `apply_action`.
- **Affected files:** `src/frontends/native/rom_browser/app.rs` (+ new siblings),
  `rom_browser/mod.rs`.
- **Acceptance criteria:** identical browser behavior (manual smoke of grid/search/filter/
  detail/favorites + gamepad nav); each new file < ~800 lines; clippy/fmt clean; existing
  browser tests pass.
- **Suggested sub-tasks:** (1) extract `texture_cache.rs`; (2) extract `favorites_state.rs`;
  (3) extract `catalog_state.rs`; (4) extract `browser_input.rs`; (5) extract
  `browser_render.rs`; (6) reduce `RomBrowserApp` to orchestration.
- **Dependencies:** none (frontend-local).
- **Suggested labels:** `refactoring`

#### I1.3 — Split NES `console/config.rs`
- **Impact:** Medium · **Effort:** Medium
- **Problem:** `src/nes/console/config.rs` is 5,575 lines (≈1,449 impl + inline tests),
  combining `Config`/`NesConfig`, CLI flag definitions, config-file loading, and hardware/
  timing/input defaults.
- **Proposed approach:** Within `src/nes/console/`, split into `config/mod.rs` (structs +
  `Config` composition), `config/cli.rs` (argument parsing/flags), `config/defaults.rs`
  (hardware/timing/input defaults + file loading). Keep public re-exports stable.
- **Affected files:** `src/nes/console/config.rs`, `src/nes/console/mod.rs`,
  `src/nes/console/nes.rs` (CLI entry).
- **Acceptance criteria:** all config tests pass unchanged; CLI flags parse identically
  (spot-check `--help` output); clippy/fmt clean.
- **Suggested sub-tasks:** (1) move structs; (2) move CLI parsing; (3) move defaults/loading.
- **Dependencies:** Consider after I1.4 to share patterns with `platform/config.rs`.
- **Suggested labels:** `refactoring`

#### I1.4 — Split `platform/config.rs` mega-config
- **Impact:** Medium · **Effort:** Medium
- **Problem:** `src/platform/config.rs` (2,134 lines) holds `FrontendConfig` plus
  cross-console settings in one file.
- **Proposed approach:** Break into `platform/config/mod.rs` + per-domain submodules
  (audio, video/window, autorun, debugger, metadata paths), keeping the parsing glue
  separate from the data structs.
- **Affected files:** `src/platform/config.rs`, `src/platform/mod.rs`.
- **Acceptance criteria:** no behavior change; `resolved_*_path()` helpers and all config
  tests pass; clippy/fmt clean.
- **Suggested sub-tasks:** (1) data structs by domain; (2) parsing/loading; (3) path helpers.
- **Dependencies:** Pairs with I1.3 (shared idioms).
- **Suggested labels:** `refactoring`

#### I1.5 — Split `frontends/native/keyboard.rs`
- **Impact:** Medium · **Effort:** Medium
- **Problem:** `src/frontends/native/keyboard.rs` (1,998 lines) mixes global hotkeys,
  per-console key dispatch (NES/GB/GBA/SNES; `keyboard.rs:50-105,235-260`), controller
  mapping tables, and inline tests.
- **Proposed approach:** Split into `keyboard/hotkeys.rs` (system/debugger hotkeys),
  `keyboard/console_keyboard.rs` (button dispatch), `keyboard/controller_mapping.rs`
  (key→button tables). De-leak console branching once I3.1 lands.
- **Affected files:** `src/frontends/native/keyboard.rs` (+ siblings),
  `src/frontends/native/mod.rs`.
- **Acceptance criteria:** identical key behavior across consoles; clippy/fmt clean; tests pass.
- **Suggested sub-tasks:** (1) extract hotkeys; (2) extract mapping tables; (3) extract dispatch.
- **Dependencies:** Cleaner after I3.1 (capability traits).
- **Suggested labels:** `refactoring`

#### I1.6 — Slim `frontends/native/event_loop.rs`
- **Impact:** Medium · **Effort:** Medium
- **Problem:** `src/frontends/native/event_loop.rs` (1,347 lines) branches on
  `Console::Nes/GameBoy/GameBoyAdvance/Snes` for paused-debugger handling and frame
  execution (`event_loop.rs:193-260`).
- **Proposed approach:** Extract frame-execution and debugger-pause handling behind the
  capability traits from I3.1, leaving a thin event loop. Move helper logic into a
  `frame_runner.rs`.
- **Affected files:** `src/frontends/native/event_loop.rs`, new `frame_runner.rs`.
- **Acceptance criteria:** behavior parity (pause/resume, debugger step, hot-reload); clippy/fmt clean.
- **Suggested sub-tasks:** (1) introduce capability calls (post-I3.1); (2) extract frame runner.
- **Dependencies:** **Depends on I3.1.**
- **Suggested labels:** `refactoring`

#### I1.7 — Catalogue remaining large impls
- **Impact:** Medium · **Effort:** Med–High
- **Problem:** Other large impl files are decomposition candidates:
  `snes/cpu/cpu.rs` (~3,960 impl), `gb/ppu/pixel_fifo.rs` (~3,998 impl),
  `gba/ppu/mod.rs` (~2,165 impl), `snes/apu/spc700/cpu.rs` (~big impl).
- **Proposed approach:** Treat each as its own follow-up issue using the I1.1 pattern
  (split by responsibility, tests move with code). Do **not** batch them — each core is
  delicate and benefits from isolated review.
- **Affected files:** as listed.
- **Acceptance criteria (per file):** no behavior change; integration suites for that core
  pass; clippy/fmt clean; resulting impl files < ~1,500 lines.
- **Suggested sub-tasks:** one issue per file, prioritized by churn.
- **Dependencies:** Pattern established by I1.1.
- **Suggested labels:** `refactoring`

---

### Area 2 — Cross-Console Abstraction & Duplication

> Goal: extract shared infrastructure into `src/platform/` so the four consoles stop
> re-implementing the same plumbing. (These land in `src/platform/` now and relocate into
> a `core`/`platform` crate later under I4.1.)

#### I2.1 — Shared `Stateful` trait + `platform::save_state` helpers
- **Impact:** High · **Effort:** Medium
- **Problem:** All four consoles hand-roll versioned serde-JSON save-states with duplicated
  version-gating and error wrapping:
  `src/nes/console/nes.rs` (~1150-1215), `src/gb/console/save_state.rs` (~147-167),
  `src/gba/console/save_state.rs` (~171-220), `src/snes/console/save_state.rs` (~354-369).
  A `Stateful` trait is already **designed in `docs/architecture-diagrams.md` but
  unimplemented** (no `trait Stateful` exists in `src/`). There is no compile-time
  enforcement that a component implements capture/restore.
- **Proposed approach:**
  1. Add `platform::save_state` with `pub trait Stateful { type State: Serialize +
     DeserializeOwned; fn capture_state(&self) -> Self::State; fn restore_state(&mut self,
     &Self::State); }` (matching the design doc).
  2. Add shared helpers: `to_bytes<T>`, `from_bytes<T>`, and `check_version(found,
     supported: &[u32]) -> Result<(), SaveStateError>` plus a common `SaveStateError`
     (pairs with I4.2).
  3. Migrate consoles one at a time to implement `Stateful` and use the shared
     version/serialize helpers.
- **Affected files:** new `src/platform/save_state.rs`; the four console save-state sites
  above; `src/platform/mod.rs`.
- **Acceptance criteria:** every existing save-state round-trip test passes; old save-state
  bytes still load (version compatibility preserved); adding a new stateful component fails
  to compile until `Stateful` is implemented; clippy/fmt clean.
- **Suggested sub-tasks:** (1) add trait+helpers+error; (2) migrate NES; (3) GB; (4) GBA;
  (5) SNES; (6) document the pattern in `architecture.md`.
- **Dependencies:** Enables I2.2; pairs with I4.2.
- **Suggested labels:** `refactoring`, `enhancement`

#### I2.2 — Evaluate save-state format migration (JSON → postcard)
- **Impact:** Medium · **Effort:** Medium
- **Problem:** Save-states use serde-JSON (`serde_json` in all four console save-state
  files), which is large and slow versus binary. `postcard` is **already a dependency and
  in use for NES autorun** (`src/nes/console/nes.rs`, `src/platform/autorun/utils.rs`), so
  the toolchain exists.
- **Proposed approach:** Behind the I2.1 helpers, add a binary encoder (postcard) selected
  by the save-state container/version. Keep JSON readers for existing states (or provide a
  one-time migration on load). Benchmark size/speed on a representative state before
  committing to a default.
- **Affected files:** `src/platform/save_state.rs`, console save-state modules.
- **Acceptance criteria:** old JSON states still load; new states round-trip via postcard;
  measured size/time improvement recorded in the PR; web (WASM) save-state path verified.
- **Suggested sub-tasks:** (1) add format tag to the versioned container; (2) postcard
  encode/decode; (3) compatibility/migration test; (4) benchmark + decide default.
- **Dependencies:** **Depends on I2.1.**
- **Suggested labels:** `enhancement`, `refactoring`

#### I2.3 — Reduce `Emulator` impl forwarding boilerplate
- **Impact:** Medium · **Effort:** Low
- **Problem:** Each console's `impl Emulator` is mostly thin forwarders to inherent methods
  (`nes.rs:1231-1315`, `gb/console/gameboy.rs:600-683`, `gba/console/gba.rs:319-460`,
  `snes/console/snes.rs:274-450`), e.g. `fn run_tick(&mut self) -> u8 { self.run_cpu_tick() }`.
- **Proposed approach:** Either (a) a declarative `impl_emulator_forwarding!` macro for the
  purely-mechanical methods, or (b) split `Emulator` into smaller traits with default
  methods where a sensible default exists. Prefer the least-magic option that keeps method
  discovery easy.
- **Affected files:** `src/platform/emulator.rs`, the four `impl Emulator` sites.
- **Acceptance criteria:** no behavior change; the four impls shrink measurably; clippy/fmt
  clean; trait object dispatch via `as_core()`/`as_core_mut()` unchanged.
- **Suggested sub-tasks:** (1) identify mechanically-identical methods; (2) introduce
  macro/sub-traits; (3) migrate the four consoles.
- **Dependencies:** none.
- **Suggested labels:** `refactoring`

#### I2.4 — Shared headless integration-test harness
- **Impact:** Medium · **Effort:** Medium
- **Problem:** The "load ROM → run N frames → framebuffer CRC → optional PNG capture gated
  by `NESER_CAPTURE_SCREEN`" pattern is duplicated:
  `src/gb/integration_tests/helpers.rs` (`run_frames_and_crc`, `save_screen_png`),
  `src/gba/integration_tests/gba_suite_runner.rs` (`maybe_write_capture_png`, ~306-317),
  and ad-hoc repeats in NES tests (`mapper_tests.rs`, `ppu_tests.rs`).
- **Proposed approach:** Add a `platform::test_support` module (compiled under `cfg(test)`
  or a `test-support` feature) exposing generic helpers over the `Emulator` trait:
  `run_frames(&mut dyn Emulator, n)`, `frame_crc(&dyn Emulator)`,
  `capture_png_if_enabled(path, &dyn Emulator)`. Refactor GB/GBA/NES harnesses to call it.
- **Affected files:** new `src/platform/test_support.rs`; the three harness files above.
- **Acceptance criteria:** all integration suites pass with identical CRCs; PNG-capture env
  gate behaves identically; no duplication of the capture predicate remains.
- **Suggested sub-tasks:** (1) add shared helpers; (2) migrate GB; (3) migrate GBA;
  (4) migrate NES; (5) wire SNES when it grows screen tests.
- **Dependencies:** Smoother after I4.2 (consistent error types in loaders).
- **Suggested labels:** `testing`, `refactoring`

#### I2.5 — Consolidate debugging primitives
- **Impact:** Medium · **Effort:** Low
- **Problem:** `platform::debugging::BreakpointList` exists
  (`src/platform/debugging/breakpoints.rs:220`) and GB already uses it, but **GBA
  re-implements its own** `Breakpoints { addrs: BTreeSet<u32> }`
  (`src/gba/debugging/breakpoints.rs:16`). Trace ring buffers are also re-implemented
  (`gba/debugging/trace.rs`).
- **Proposed approach:** Make the shared `BreakpointList` generic over address width (or
  add a `u32` variant) and replace GBA's `Breakpoints`. Extract a reusable trace ring
  buffer into `platform::debugging` and have GBA (and future SNES) use it.
- **Affected files:** `src/platform/debugging/breakpoints.rs`,
  `src/gba/debugging/breakpoints.rs`, `src/gba/debugging/trace.rs`,
  `src/gba/debugging/controller.rs`.
- **Acceptance criteria:** GBA debugger behavior unchanged; GBA's bespoke breakpoint set
  removed; debugging tests pass; clippy/fmt clean.
- **Suggested sub-tasks:** (1) generalize `BreakpointList`; (2) swap GBA breakpoints;
  (3) extract shared trace ring buffer; (4) swap GBA trace.
- **Dependencies:** none.
- **Suggested labels:** `refactoring`

#### I2.6 — Shared input primitives (serial / shift-register)
- **Impact:** Medium · **Effort:** Med–High
- **Problem:** Each console re-implements serial/shift-register controller state and
  button-id mapping: NES `Controller` trait (`src/nes/input/controller.rs:116-178`), SNES
  `SnesController` trait (`src/snes/input/mod.rs:160-257`), plus GB/GBA register variants.
  NES and SNES are especially parallel (strobe + shift-out + capture/restore).
- **Proposed approach:** Add `platform::input` with a reusable shift-register controller
  helper (latch/strobe/shift, capture/restore) and button-id/bitmask utilities. Keep
  console-specific register/IRQ semantics in thin per-console adapters that compose the
  shared core.
- **Affected files:** new `src/platform/input.rs`; `src/nes/input/`, `src/snes/input/`,
  `src/gb/input/`, `src/gba/input/`.
- **Acceptance criteria:** all input tests pass; controller serial timing unchanged;
  save-state of input devices round-trips; clippy/fmt clean.
- **Suggested sub-tasks:** (1) shared shift-register core + tests; (2) migrate NES joypad;
  (3) migrate SNES standard controller; (4) evaluate GB/GBA reuse.
- **Dependencies:** Pairs with I2.1 (input save-state via `Stateful`).
- **Suggested labels:** `refactoring`

---

### Area 3 — Frontend Coupling

#### I3.1 — Capability traits to remove `Console::` frontend leakage
- **Impact:** High · **Effort:** Med–High
- **Problem:** Frontends are supposed to program against the `Emulator` trait, but **85
  `Console::<Variant>` match sites** leak console-specific behavior across
  `src/frontends/native/{mouse,keyboard,event_loop,gl_backend,app_state,gamepad}.rs` and
  `src/frontends/web/wasm.rs`. Examples: NES/SNES mouse routing (`mouse.rs:24-160`),
  per-console key dispatch (`keyboard.rs:50-105`), debugger/PPU-viewer overlays gated on
  `Console::Nes` (`gl_backend.rs`), NES-only debugger/PPU helpers in WASM (`wasm.rs`).
- **Proposed approach:** Define **capability traits** for optional features and let consoles
  opt in:
  - `MouseInput` (zapper / SNES-mouse / paddle motion + buttons + crosshair),
  - `Debuggable` (debugger controller access),
  - `PpuViewer` (nametable/pattern viewers).
  Expose `Console`-level accessors returning `Option<&dyn Trait>` /
  `Option<&mut dyn Trait>` so frontends ask "does this console support X?" via the trait
  instead of matching variants. Migrate the leak sites to the new accessors.
- **Affected files:** `src/platform/emulator.rs` (+ new capability traits),
  `src/frontends/native/{mouse,keyboard,event_loop,gl_backend,app_state}.rs`,
  `src/frontends/web/wasm*.rs`, each console's `console/*.rs`.
- **Acceptance criteria:** `Console::<Variant>` match count in `src/frontends/` drops
  substantially (target: only construction/identity sites remain); feature parity for
  debugger, zapper, SNES mouse, PPU viewer on native and web; clippy/fmt clean.
- **Suggested sub-tasks:** (1) define traits; (2) implement for NES; (3) implement where
  applicable for GB/GBA/SNES; (4) migrate native frontend; (5) migrate web frontend.
- **Dependencies:** Unblocks I1.5 and I1.6.
- **Suggested labels:** `refactoring`, `enhancement`

#### I3.2 — Deduplicate WASM bindings; add `WasmSnes`
- **Impact:** Medium · **Effort:** Low–Med
- **Problem:** `wasm.rs` (NES, ~1,242 lines), `wasm_gb.rs` (~193), `wasm_gba.rs` (~255)
  repeat the same JS-facing surface (init, `load_rom`, `drain_toasts`,
  `render_frame_rgba`, `screen_width/height`, `frame_rate_hz`, audio getters/mute/rate,
  `set_button`, `reset`, save/load state). There is **no `WasmSnes`** despite a SNES core.
- **Proposed approach:** Extract the identical boilerplate into a shared module or a
  `wasm_bridge_common!` macro / `WasmEmulator<T>` adapter over `Console`/`Emulator`. Add a
  `WasmSnes` binding once the common surface exists. Keep NES-only debugger/mouse helpers as
  an extension on the NES binding.
- **Affected files:** `src/frontends/web/wasm.rs`, `wasm_gb.rs`, `wasm_gba.rs`,
  new `wasm_common.rs`/`wasm_snes.rs`; `web/src/app.ts` (console selection),
  `web/src/rom/rom_extensions.ts`.
- **Acceptance criteria:** `wasm-pack test --headless --chrome --no-default-features
  --features wasm` passes; existing NES/GB/GBA browser behavior unchanged; SNES ROM loads
  and renders in the browser; Playwright web tests pass.
- **Suggested sub-tasks:** (1) extract common surface; (2) migrate GB/GBA; (3) migrate NES
  (keep NES extras); (4) add `WasmSnes`; (5) wire `.sfc/.smc` detection.
- **Dependencies:** Benefits from I3.1 (capabilities expose NES-only extras cleanly).
- **Suggested labels:** `refactoring`, `enhancement`

#### I3.3 — Shared logical-input layer (native ↔ web)
- **Impact:** Medium · **Effort:** Medium
- **Problem:** Native (Rust: `keyboard.rs`, `gamepad.rs`, `mouse.rs`) and web (TypeScript:
  `web/src/input/*`) maintain parallel, manually-synced mapping tables for the same logical
  buttons, risking drift.
- **Proposed approach:** Define a single **logical input vocabulary**
  (`Up/Down/Left/Right/A/B/Start/Select/L/R/X/Y` + mouse actions) and centralize default
  mapping tables. Where practical, generate the TS tables from one Rust source (or document
  a single canonical table both mirror) to prevent divergence.
- **Affected files:** `src/frontends/native/{keyboard,gamepad,mouse}.rs`,
  `web/src/input/{keyboard_mapping,gamepad,mouse_input}.ts`.
- **Acceptance criteria:** identical default bindings on native and web; a single source of
  truth for default maps; Rust + Vitest input tests pass.
- **Suggested sub-tasks:** (1) define logical-input enum; (2) centralize native defaults;
  (3) align web tables / generation; (4) regression tests both sides.
- **Dependencies:** Cleaner after I1.5.
- **Suggested labels:** `refactoring`

---

### Area 4 — Build / Workspace / Errors

#### I4.1 — Phased Cargo workspace split
- **Impact:** High · **Effort:** High
- **Problem:** The whole project is a single 382K-line crate (`Cargo.toml`, no
  `[workspace]`) whose `default = ["native"]` pulls heavy GUI/audio/net/db deps (`winit`,
  `glutin`, `egui*`, `librashader`, `cpal`, `gilrs`, `reqwest`, `rusqlite`, `image`). This
  hurts incremental compile time and provides no enforced layering between emulation cores
  and platform/frontends.
- **Proposed approach (phased, non-breaking each step):**
  1. **Leaf core:** carve a `neser-core` crate with the four emulation cores and zero GUI/
     net deps (only `serde`, `bitflags`, `crc`, `rand`, `postcard`, …).
  2. **Platform:** a `neser-platform` crate (config, catalog, metadata, image cache, audio,
     autorun) depending on `neser-core`.
  3. **Frontends:** `neser-native`, `neser-web`, `neser-tui` crates depending on platform,
     each owning their optional deps; the `neser` binary lives in `neser-native`.
  Convert to a virtual workspace; migrate module paths incrementally; keep CI green at each
  step.
- **Affected files:** `Cargo.toml` → workspace + member manifests; broad `use` path updates;
  `build.rs`; CI workflows.
- **Acceptance criteria:** `cargo build`/`test` per crate succeed; WASM build/test pass;
  native binary runs; measured incremental-build improvement recorded; no feature/behavior
  regressions.
- **Suggested sub-tasks:** (1) extract `neser-core`; (2) extract `neser-platform`;
  (3) split frontends; (4) update CI path filters per crate; (5) update docs.
- **Dependencies:** Later track; shared abstractions (I2.x, I3.1) land in `src/platform/`
  first and **relocate** into `neser-platform`/`neser-core` here.
- **Suggested labels:** `refactoring`, `enhancement`

#### I4.2 — Typed errors at public/core boundaries
- **Impact:** High · **Effort:** Med–High
- **Problem:** 189 `Result<_, String>` boundaries, including the entire `Emulator` trait
  (`load_rom`, `save_state_bytes`, `load_state_bytes`, `save_ram` in
  `src/platform/emulator.rs:34,57,58,60`). No `thiserror`/`anyhow`; only 6 ad-hoc error
  enums. Stringly errors are hard to match on and lose structure across the WASM boundary.
- **Proposed approach:** Adopt `thiserror` for boundary error enums: `LoadRomError`,
  `SaveStateError`, `SaveRamError` (and a `FrontendError` aggregate). Change the `Emulator`
  trait signatures to typed errors with `Display` impls so frontends (incl. WASM, which
  needs `String` at the JS edge) can convert at the very edge. Keep `unwrap`/`expect`/
  `panic!` in tests and proven-invariant hot paths (do not sweep those).
- **Affected files:** new `src/platform/error.rs`; `src/platform/emulator.rs`; each
  console's `load_rom`/save-state/save-ram surfaces; `src/frontends/web/wasm*.rs`
  (convert to `JsValue`/`String` at the edge).
- **Acceptance criteria:** `Emulator` trait uses typed errors; callers compile; WASM still
  surfaces readable messages to JS; tests pass; clippy/fmt clean.
- **Suggested sub-tasks:** (1) add `thiserror` + error enums; (2) migrate `Emulator` trait;
  (3) migrate per-console impls; (4) adapt frontends; (5) document the boundary rule.
- **Dependencies:** Pairs with I2.1 (`SaveStateError`).
- **Suggested labels:** `refactoring`, `enhancement`

#### I4.3 — Audit `dead_code` and `too_many_arguments` allows
- **Impact:** Low–Med · **Effort:** Medium
- **Problem:** 85 `#[allow(dead_code)]` (concentrated in `nes/console/nes.rs` ×8,
  `nes/cartridge/common.rs` ×5, `nes/bus/bus.rs` ×5, …) and 30
  `#[allow(clippy::too_many_arguments)]` across CPU/PPU/DMA hot code. Some `dead_code`
  may be genuinely unused; arg-heavy functions are a readability smell.
- **Proposed approach:** For each `dead_code` allow, either delete the unused item or, if
  it's an intentional future/API hook, document why and gate it appropriately. For
  `too_many_arguments`, group related parameters into small `Copy` context structs (e.g.
  fetch/render context) rather than blanket-allowing.
- **Affected files:** the `nes/`, `gba/`, `snes/`, `gb/`, `frontends/` sites listed by
  `grep -rn '#\[allow(dead_code)\]'` / `too_many_arguments`.
- **Acceptance criteria:** net reduction in `#[allow(...)]` count; no behavior change;
  clippy/fmt clean; tests pass.
- **Suggested sub-tasks:** (1) triage dead_code (delete vs justify); (2) introduce param
  structs for the worst arg offenders; (3) remove the now-unneeded allows.
- **Dependencies:** Easier per-area after the relevant Wave-2 file splits.
- **Suggested labels:** `refactoring`

---

### Area 5 — Tooling / CI / Python

#### I5.1 — CI caching + integration-suite parallelization
- **Impact:** Medium · **Effort:** Low–Med
- **Problem:** Integration tests are ~97% of test time. CI (`.github/workflows/ci.yml`)
  already uses `dorny/paths-filter` and `Swatinem/rust-cache`, but the web job re-installs
  `wasm-bindgen-cli`, Playwright, browsers, and npm deps every run (`ci.yml:261-300`), and
  integration suites aren't maximally parallelized.
- **Proposed approach:** Cache `cargo install wasm-bindgen-cli` (or pin a prebuilt binary);
  cache Playwright browsers; split the heavy integration suites into a matrix of jobs so
  they run concurrently. Consider a "fast PR" workflow (unit + lint + fmt) separate from a
  "full" main/nightly workflow.
- **Affected files:** `.github/workflows/ci.yml`, possibly `release.yml`.
- **Acceptance criteria:** measurable CI wall-clock reduction; same coverage; no flakiness
  introduced; cache keys correct.
- **Suggested sub-tasks:** (1) cache wasm-bindgen + Playwright; (2) matrix-split integration
  suites; (3) optional fast-PR workflow.
- **Dependencies:** I4.1 may later reshape path filters (keep them in sync).
- **Suggested labels:** `testing`, `enhancement`

#### I5.2 — Python tooling baseline
- **Impact:** Medium · **Effort:** Low–Med
- **Problem:** Python tools (`scripts/`, `scripts/mappertool/`, `scripts/metadata_scraper/`,
  `scripts/nes_rom_db_scraper/`) have only `requirements-*.txt`; no `pyproject.toml`, no
  `ruff`/`black`/`mypy`/`pyright` config. Tests run via `unittest discover`.
- **Proposed approach:** Add a `scripts/pyproject.toml` configuring `ruff` (lint + format)
  and pinning test deps; keep `unittest` (or adopt `pytest`). Add type hints incrementally
  to shared modules (`api_client.py`, `metadata_db.py`, `rom_database.py`). Document the
  venv/setup once.
- **Affected files:** new `scripts/pyproject.toml`; CI Python job (`ci.yml:93-99`); script
  modules (incremental hints).
- **Acceptance criteria:** `ruff check` clean (or baseline-ignored); existing Python tests
  still pass in CI; documented setup.
- **Suggested sub-tasks:** (1) add pyproject + ruff; (2) wire CI lint step; (3) incremental
  type hints on shared modules.
- **Dependencies:** none.
- **Suggested labels:** `testing`, `refactoring`

#### I5.3 — Document vendor submodule cadence
- **Impact:** Low · **Effort:** Low
- **Problem:** `vendor/slang-shaders` is a large vendored submodule (`.gitmodules`); release
  packaging depends on shader-preset reachability, so silent upstream drift can affect
  release builds.
- **Proposed approach:** Add a short "vendor refresh" note (update cadence, how to bump the
  submodule, how to re-verify reachable presets) to `docs/` or the release docs.
- **Affected files:** `.gitmodules` (reference), a short `docs/` note,
  `scripts/package_release.py` (reference).
- **Acceptance criteria:** a documented, repeatable submodule-update process exists.
- **Suggested sub-tasks:** (1) write the note; (2) link it from README/release docs.
- **Dependencies:** none.
- **Suggested labels:** `enhancement`

---

### Area 6 — Documentation Hygiene

#### I6.1 — Fix `architecture.md` drift
- **Impact:** Medium · **Effort:** Low
- **Problem:** `architecture.md` is partly stale: it claims "~183,000 lines"
  (`architecture.md:11`; reality ≈382K) and refers to an `sdl` feature
  (`architecture.md:84`; reality `native`). The save-state section says "JSON" without
  noting the partial postcard usage.
- **Proposed approach:** Correct the line-count, feature names (`native`/`wasm`/`tui`), and
  save-state format description. Add the `Stateful` trait once I2.1 lands.
- **Affected files:** `architecture.md` (and `docs/architecture-diagrams.md` cross-ref).
- **Acceptance criteria:** no factual mismatches between `architecture.md` and
  `Cargo.toml`/source for the cited items.
- **Suggested sub-tasks:** (1) fix metrics/feature names; (2) update save-state section.
- **Dependencies:** none (do early; refresh again after big refactors).
- **Suggested labels:** `enhancement`

#### I6.2 — Consolidate README sprawl
- **Impact:** Low–Med · **Effort:** Low
- **Problem:** Four overlapping top-level READMEs (`README.md` 206, `README-NES.md` 156,
  `README-GB.md` 132, `README-GBA.md` 138) duplicate setup/build/test instructions, which
  drift independently.
- **Proposed approach:** Make `README.md` a thin index/overview linking to per-console
  pages, and keep one canonical "build & test" section (referenced, not copied) — ideally
  pointing at the checkpoint commands so they stay authoritative.
- **Affected files:** `README.md`, `README-NES.md`, `README-GB.md`, `README-GBA.md`.
- **Acceptance criteria:** build/test instructions exist in exactly one canonical place;
  per-console READMEs cover only console-specific content; links valid.
- **Suggested sub-tasks:** (1) extract canonical build/test; (2) slim per-console READMEs;
  (3) fix cross-links.
- **Dependencies:** none.
- **Suggested labels:** `enhancement`

#### I6.3 — Add a doc-sync check
- **Impact:** Low · **Effort:** Low
- **Problem:** Doc drift (I6.1) recurs because nothing flags `architecture.md` ↔
  `Cargo.toml` mismatches.
- **Proposed approach:** Add a lightweight check (script or CI step) that fails when, e.g.,
  feature names referenced in `architecture.md` don't exist in `Cargo.toml`, or when a
  declared LOC figure is wildly off. Keep it cheap and advisory.
- **Affected files:** new `scripts/check_docs_sync.py` (or shell), `.github/workflows/ci.yml`.
- **Acceptance criteria:** the check catches an intentionally-introduced mismatch in a test;
  runs fast in CI.
- **Suggested sub-tasks:** (1) write the check; (2) wire into CI as non-blocking → blocking.
- **Dependencies:** Follows I6.1; pairs with I5.2 Python baseline.
- **Suggested labels:** `testing`, `enhancement`

---

## 5. Non-Goals (deliberate exclusions)

- **Relocating inline `#[cfg(test)]` modules out of source files.** Inline unit tests are
  idiomatic Rust and keep tests close to code. Decomposition (Area 1) splits **impl** by
  responsibility and moves each test cluster *with* the code it covers — it does not exile
  tests into separate files.
- **Fully unifying the per-console CPU bus traits.** `GbBus`, the GBA `Bus`, and `SnesBus`
  diverge meaningfully (8-bit cycle-annotated vs width-based vs NES's concrete struct). A
  single grand bus trait would be a leaky abstraction for low payoff. (A *thin* shared
  read/write/tick helper could be revisited opportunistically, but it is not recommended as
  a standalone initiative.)

---

## 6. Appendix — Metrics Snapshot

Captured at authoring time for traceability (re-measure before acting on any item):

| Metric | Value |
|--------|-------|
| Rust source files | 659 |
| Total Rust lines | 382,032 |
| `#[test]` functions | ~11,771 |
| `wasm_bindgen_test` functions | 66 |
| `TODO/FIXME/HACK/XXX` markers | 13 |
| `#[allow(...)]` total | 199 (85 `dead_code`, 50 `unused_imports`, 30 `too_many_arguments`, 17 `module_inception`) |
| `.unwrap()` | 828 |
| `.expect(` | 1,339 |
| `panic!` | 103 |
| `unsafe` blocks | 44 |
| `Result<_, String>` boundaries | 189 |
| `Console::<Variant>` matches in `src/frontends/` | 85 |
| `serde_json` usages | 88 · `postcard` usages | 6 |

**Largest source files (lines, impl + inline tests):**

| File | Lines |
|------|-------|
| `src/nes/cpu/cpu.rs` | 16,847 |
| `src/snes/cpu/cpu.rs` | 10,489 |
| `src/gb/ppu/pixel_fifo.rs` | 7,259 |
| `src/gba/ppu/mod.rs` | 6,573 |
| `src/nes/console/config.rs` | 5,575 |
| `src/snes/apu/spc700/cpu.rs` | 5,464 |
| `src/nes/cartridge/nintendo/mmc5.rs` | 4,132 |
| `src/frontends/native/rom_browser/app.rs` | 3,790 |
| `src/nes/console/nes.rs` | 3,242 |
| `src/nes/bus/bus.rs` | 3,022 |
| `src/platform/config.rs` | 2,134 |
| `src/frontends/native/keyboard.rs` | 1,998 |
| `src/frontends/native/event_loop.rs` | 1,347 |

**Per-console test distribution:** NES 7,229 · GB 1,432 · GBA 1,102 · SNES 1,132 ·
platform 422 · frontends 429.
