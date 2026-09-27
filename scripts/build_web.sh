#!/bin/sh
set -e

# Always run from the repository root: cargo would find the workspace from any
# cwd, but the wasm-bindgen/web/pkg/vite paths below are root-relative.
cd "$(dirname "$0")/.."

# rustup's cargo proxy exports RUSTUP_TOOLCHAIN to every process it starts, so a shell opened from
# anything launched with `cargo run` (the Cerebro fleet view, for one) inherits `stable`, which
# beats rust-toolchain.toml. Unset it so this runs on the pinned toolchain, as CI does.
unset RUSTUP_TOOLCHAIN

# --no-bundle builds web/pkg (the wasm and its bindings, types included) and stops before vite:
# the pre-merge gate type-checks against those bindings without bundling the app (nr-n48).
BUNDLE=1
if [ "${1:-}" = "--no-bundle" ]; then
    BUNDLE=0
fi

# Only skip the WASM build when explicitly requested (e.g. in CI with pre-built artifacts)
SKIP_WASM_BUILD_IF_ARTIFACTS_EXIST="${SKIP_WASM_BUILD_IF_ARTIFACTS_EXIST:-0}"
if [ "$SKIP_WASM_BUILD_IF_ARTIFACTS_EXIST" = "1" ] && [ -f web/pkg/neser_bg.wasm ] && [ -f web/pkg/neser.js ]; then
    :
else
    # wasm-bindgen from PATH, or wasm-pack's cached copy of the pinned version when PATH has none.
    WASM_BINDGEN="$(sh scripts/find_wasm_bindgen.sh)"
    cargo build --profile wasm-release --target wasm32-unknown-unknown --no-default-features --features wasm
    "$WASM_BINDGEN" target/wasm32-unknown-unknown/wasm-release/neser.wasm --out-dir web/pkg --target web --omit-default-module-path
    npx wasm-opt -O3 web/pkg/neser_bg.wasm -o web/pkg/neser_bg.wasm
fi

if [ "$BUNDLE" = "1" ]; then
    npx vite build
fi
