# NESER Web (WASM)

## Prerequisites
- Rust toolchain with `wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown`)
- `wasm-bindgen-cli` (or `wasm-pack`) installed, or use `cargo install wasm-bindgen-cli`
- Any static file server (e.g., `python -m http.server`)

## Build
```bash
# Dev build
cargo build --release --target wasm32-unknown-unknown --no-default-features --features wasm
wasm-bindgen target/wasm32-unknown-unknown/release/neser.wasm --out-dir web/pkg --target web

# Production-lean build (smaller/faster output)
wasm-bindgen target/wasm32-unknown-unknown/release/neser.wasm --out-dir web/pkg --target web --omit-default-module-path --no-typescript

# Optional post-pass (requires Binaryen)
# wasm-opt -O3 web/pkg/neser_bg.wasm -o web/pkg/neser_bg.wasm
```

## Run locally
```bash
cd web
python -m http.server 8000
# then open http://localhost:8000 in your browser
```

## Notes
- Generated artifacts under `web/pkg/` are ignored in git; regenerate locally via the steps above.
- Keyboard input is supported for controller 1 using keys: W/A/S/D (directional), F (A button), G (B button), R (Select), T (Start).
- Optional gamepad input uses the Gamepad API (toggle in UI).
- Audio is supported via Web Audio.
- Rendering runs on the browser main thread; heavy frames or slow hosts can briefly block UI/event handling.
