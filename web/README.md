# NESER Web (WASM)

## Prerequisites
- Rust toolchain with `wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown`)
- `wasm-bindgen-cli` (or `wasm-pack`) installed, or use `cargo install wasm-bindgen-cli`
- Node.js 22.13+ with pnpm (`corepack enable` gives you the version `package.json` pins)

Install the JavaScript dependencies once, and again whenever `package.json` changes:
```bash
pnpm install
```

## Build
```bash
# The wasm and the bundle: web/pkg, then dist/
pnpm run build

# Only the wasm (web/pkg, its TypeScript types included), without bundling
pnpm run build:wasm
```

`build:wasm` compiles the `wasm-release` profile, runs `wasm-bindgen` and then `wasm-opt -O3`.
wasm-bindgen writes the bindings' TypeScript declarations to `web/pkg/neser.d.ts`, and `app.ts`'s
`../pkg/neser` import is typed from that file; nothing under `web/types/` restates them.

## Run locally
```bash
# Build, then serve dist/ at http://localhost:8000
pnpm start

# Or serve an existing build only
pnpm run serve

# Vite's development server (needs web/pkg from `pnpm run build:wasm`)
pnpm run dev
```

## Testing
```bash
# Unit tests (Vitest)
pnpm test

# Type-check (needs web/pkg, so build it first; Vite and Vitest do not check types)
pnpm run build:wasm && pnpm run typecheck

# Integration tests (Playwright)
pnpm run test:integration:web
```

The integration suite builds the app with `pnpm run build` and serves it with `pnpm run serve` on
port 8000, or on `NESER_WEB_PORT` when that is set; with `NESER_WEB_PREBUILT=1` it serves the
`dist/` already there without building. A fleet session runs it
as `.cerebro/cerebro/scripts/smoke-port -- pnpm run test:integration:web`, which gives each session
its own port (`port_base`/`port_env` in `.cerebro/project.conf`) so it never tests another
session's server. `build:wasm` runs `wasm-bindgen` from `PATH`, or else wasm-pack's cached copy
of the version `Cargo.lock` pins (`scripts/find_wasm_bindgen.sh`). The browser the suite needs is
installed with `pnpm exec playwright install --only-shell chromium`, which a prepared worktree has
already run.

## GBA performance benchmark
Build the WASM package, copy a local GBA ROM into `web/roms/`, start the dev
server, then open the benchmark page:

```bash
pnpm run build:wasm
cp roms/games/metroid-zero-mission.gba web/roms/
pnpm run dev
# open http://localhost:8000/gba-bench.html?rom=metroid-zero-mission.gba
```

Optional query parameters:
- `frames` (default `600`)
- `warmup` (default `60`)
- `stabilityRuns` (default `5`)
- `includeBiosIntro=true` to benchmark from power-on instead of skipping BIOS intro
- `continueStabilityRuns=true` to keep advancing instead of reloading the ROM
  before each stability run

## Project structure
```
web/
├── index.html              # Entry point
├── gba-bench.html          # GBA WASM frame benchmark
├── styles.css              # Custom styles
├── pkg/                    # Generated WASM artifacts (git-ignored)
├── src/                    # Application modules
│   ├── app.js              # Main entry point
│   ├── audio/              # Audio resampling, frame timing
│   ├── benchmark/          # Benchmark parsing and frame statistics
│   ├── debugger/           # Debugger UI components
│   ├── display/            # Canvas, zoom, cursor management
│   ├── input/              # Gamepad, mouse, keyboard input
│   ├── rom/                # ROM loading, autorun
│   ├── save-state/         # Save/load state management
│   ├── shortcuts/          # Keyboard shortcuts
│   └── ui/                 # Toast overlays, the control bar helpers, settings panel and library
└── integration/            # Playwright e2e tests
```
- Optional gamepad input uses the Gamepad API (toggle in UI).
- Audio is supported via Web Audio.
- Rendering runs on the browser main thread; heavy frames or slow hosts can briefly block UI/event handling.
