# NESER Web (WASM)

## Prerequisites
- Rust toolchain with `wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown`)
- `wasm-bindgen-cli` (or `wasm-pack`) installed, or use `cargo install wasm-bindgen-cli`
- Node.js 20+ with npm

## Build
```bash
# 1. Build WASM
cargo build --release --target wasm32-unknown-unknown --no-default-features --features wasm
wasm-bindgen target/wasm32-unknown-unknown/release/neser.wasm --out-dir web/pkg --target web --omit-default-module-path

# 2. Bundle with Vite (outputs to dist/)
npx vite build

# Or use the convenience script which does both:
bash scripts/build_web.sh
# or only step 1 (web/pkg, its TypeScript types included), without bundling:
bash scripts/build_web.sh --no-bundle
```

wasm-bindgen writes the bindings' TypeScript declarations to `web/pkg/neser.d.ts`, and `app.ts`'s
`../pkg/neser` import is typed from that file; nothing under `web/types/` restates them.

## Run locally
```bash
# Development server with hot reload
npm run dev

# Or production preview (after build)
bash scripts/run_web.sh
# then open http://localhost:8000 in your browser
```

## Testing
```bash
# Unit tests (Vitest)
npm test

# Type-check (needs web/pkg, so build it first; Vite and Vitest do not check types)
bash scripts/build_web.sh --no-bundle && npx tsc --noEmit -p tsconfig.json

# Integration tests (Playwright)
npm run test:integration:web
```

The integration suite builds the app with `scripts/build_web.sh` and serves it with
`scripts/run_web.sh` on port 8000, or on `NESER_WEB_PORT` when that is set. A fleet session runs it
as `.cerebro/cerebro/scripts/smoke-port -- npm run test:integration:web`, which gives each session
its own port (`port_base`/`port_env` in `.cerebro/project.conf`) so it never tests another
session's server. `build_web.sh` runs `wasm-bindgen` from `PATH`, or else wasm-pack's cached copy
of the version `Cargo.lock` pins (`scripts/find_wasm_bindgen.sh`). The browser the suite needs is
installed with `npx playwright install --only-shell chromium`, which a prepared worktree has already
run.

## GBA performance benchmark
Build the WASM package, copy a local GBA ROM into `web/roms/`, start the dev
server, then open the benchmark page:

```bash
bash scripts/build_web.sh
cp roms/games/metroid-zero-mission.gba web/roms/
npm run dev
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
│   └── ui/                 # Toast overlays, sine scroller
└── integration/            # Playwright e2e tests
```
- Optional gamepad input uses the Gamepad API (toggle in UI).
- Audio is supported via Web Audio.
- Rendering runs on the browser main thread; heavy frames or slow hosts can briefly block UI/event handling.
