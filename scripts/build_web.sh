#!/bin/sh
set -e

# Always run from the repository root: cargo would find the workspace from any
# cwd, but the wasm-bindgen/web/pkg/vite paths below are root-relative.
cd "$(dirname "$0")/.."

# rustup's cargo proxy exports RUSTUP_TOOLCHAIN to every process it starts, so a shell opened from
# anything launched with `cargo run` (the Cerebro fleet view, for one) inherits `stable`, which
# beats rust-toolchain.toml. Unset it so this runs on the pinned toolchain, as CI does.
unset RUSTUP_TOOLCHAIN

# Only skip the WASM build when explicitly requested (e.g. in CI with pre-built artifacts)
SKIP_WASM_BUILD_IF_ARTIFACTS_EXIST="${SKIP_WASM_BUILD_IF_ARTIFACTS_EXIST:-0}"
if [ "$SKIP_WASM_BUILD_IF_ARTIFACTS_EXIST" = "1" ] && [ -f web/pkg/neser_bg.wasm ] && [ -f web/pkg/neser.js ]; then
    :
else
    cargo build --profile wasm-release --target wasm32-unknown-unknown --no-default-features --features wasm
    wasm-bindgen target/wasm32-unknown-unknown/wasm-release/neser.wasm --out-dir web/pkg --target web --omit-default-module-path --no-typescript
    npx wasm-opt -O3 web/pkg/neser_bg.wasm -o web/pkg/neser_bg.wasm
fi

npx vite build



