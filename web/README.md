# NESER Web (WASM)

## Prerequisites
- Rust toolchain with `wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown`)
- `wasm-bindgen-cli` (or `wasm-pack`) installed, or use `cargo install wasm-bindgen-cli`
- Any static file server (e.g., `python -m http.server`)

## Build
```bash
cargo build --release --target wasm32-unknown-unknown --no-default-features --features wasm
wasm-bindgen target/wasm32-unknown-unknown/release/neser.wasm --out-dir web/pkg --target web
```

## Run locally
```bash
cd web
python -m http.server 8000
# then open http://localhost:8000 in your browser
```

## Notes
- Generated artifacts under `web/pkg/` are ignored in git; regenerate locally via the steps above.
- The MVP is graphics-only: no audio or input.
