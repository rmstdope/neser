#!/bin/sh

cargo build --release --target wasm32-unknown-unknown --no-default-features --features wasm

wasm-bindgen target/wasm32-unknown-unknown/release/neser.wasm --out-dir web/pkg --target web --omit-default-module-path --no-typescript

# wasm-opt -O3 web/pkg/neser_bg.wasm -o web/pkg/neser_bg.wasm



