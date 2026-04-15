#!/bin/sh
set -e

# Skip WASM build if artifacts already exist (e.g. pre-built in CI)
if [ ! -f web/pkg/neser_bg.wasm ] || [ ! -f web/pkg/neser.js ]; then
    cargo build --release --target wasm32-unknown-unknown --no-default-features --features wasm
    wasm-bindgen target/wasm32-unknown-unknown/release/neser.wasm --out-dir web/pkg --target web --omit-default-module-path --no-typescript
fi

# wasm-opt -O3 web/pkg/neser_bg.wasm -o web/pkg/neser_bg.wasm

npx vite build



