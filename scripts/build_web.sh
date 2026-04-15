#!/bin/sh
set -e

# Only skip the WASM build when explicitly requested (e.g. in CI with pre-built artifacts)
SKIP_WASM_BUILD_IF_ARTIFACTS_EXIST="${SKIP_WASM_BUILD_IF_ARTIFACTS_EXIST:-0}"
if [ "$SKIP_WASM_BUILD_IF_ARTIFACTS_EXIST" = "1" ] && [ -f web/pkg/neser_bg.wasm ] && [ -f web/pkg/neser.js ]; then
    :
else
    cargo build --release --target wasm32-unknown-unknown --no-default-features --features wasm
    wasm-bindgen target/wasm32-unknown-unknown/release/neser.wasm --out-dir web/pkg --target web --omit-default-module-path --no-typescript
fi

# wasm-opt -O3 web/pkg/neser_bg.wasm -o web/pkg/neser_bg.wasm

npx vite build



