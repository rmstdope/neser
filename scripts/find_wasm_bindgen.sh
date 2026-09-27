#!/bin/sh
# Prints the path of the wasm-bindgen CLI the web build should run, or exits 1 naming the version
# it needs.
#
# The CLI must match the wasm-bindgen crate version Cargo.lock pins. It is taken from PATH when
# there is one there (CI installs the pinned version on PATH); otherwise from wasm-pack's cache,
# where `wasm-pack test` leaves a copy on every machine that runs the gate, as long as that
# copy's `--version` is the pinned one. WASM_PACK_CACHE overrides the cache location, as it does
# for wasm-pack itself.
set -e

root="$(cd "$(dirname "$0")/.." && pwd)"

if command -v wasm-bindgen >/dev/null 2>&1; then
    command -v wasm-bindgen
    exit 0
fi

version="$(sed -n '/^name = "wasm-bindgen"$/{n;s/^version = "\(.*\)"$/\1/p;}' "$root/Cargo.lock")"

if [ -n "$WASM_PACK_CACHE" ]; then
    cache="$WASM_PACK_CACHE"
elif [ "$(uname)" = "Darwin" ]; then
    cache="$HOME/Library/Caches/.wasm-pack"
else
    cache="${XDG_CACHE_HOME:-$HOME/.cache}/.wasm-pack"
fi

for candidate in "$cache"/wasm-bindgen-*/wasm-bindgen; do
    [ -x "$candidate" ] || continue
    if [ "$("$candidate" --version 2>/dev/null)" = "wasm-bindgen $version" ]; then
        echo "$candidate"
        exit 0
    fi
done

echo "find_wasm_bindgen.sh: no wasm-bindgen $version on PATH or in $cache; install it with 'cargo install wasm-bindgen-cli --version $version' or run 'wasm-pack test' once" >&2
exit 1
