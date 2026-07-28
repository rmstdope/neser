#!/usr/bin/env bash
#
# Run Rust tests for specific source directories.
#
# Converts directory paths under src/ to Rust module filters and passes them
# to `cargo test --lib`. Only test execution is filtered; the full crate is
# still compiled.
#
# Usage:
#   ./scripts/test-dir.sh src/nes/cartridge          # run cartridge tests
#   ./scripts/test-dir.sh src/gb src/platform         # run gb + platform tests
#   ./scripts/test-dir.sh src/nes --skip-integration  # nes unit tests only
#
# Options:
#   --skip-integration   Exclude nes/gb/gba/snes integration_tests modules
#   --list               List matching tests without running them
#   --                   Pass remaining args directly to cargo test
#
# Environment:
#   CARGO_TEST_ARGS      Extra arguments passed to cargo test (default: --no-default-features)

set -euo pipefail

DIRS=()
SKIP_INTEGRATION=false
LIST_ONLY=false
EXTRA_ARGS=()
PASSTHROUGH=false

for arg in "$@"; do
    if $PASSTHROUGH; then
        EXTRA_ARGS+=("$arg")
        continue
    fi
    case "$arg" in
        --skip-integration)
            SKIP_INTEGRATION=true
            ;;
        --list)
            LIST_ONLY=true
            ;;
        --)
            PASSTHROUGH=true
            ;;
        --help|-h)
            head -20 "$0" | grep '^#' | sed 's/^# \?//'
            exit 0
            ;;
        *)
            DIRS+=("$arg")
            ;;
    esac
done

if [ ${#DIRS[@]} -eq 0 ]; then
    echo "Usage: $0 <dir> [<dir>...] [--skip-integration] [--list] [-- <cargo args>]" >&2
    echo "Example: $0 src/nes/cartridge" >&2
    exit 1
fi

# Convert directory paths to Rust module filters.
# src/nes/cartridge/ → nes::cartridge::
# Trailing :: ensures the filter matches only that module (avoids "gb" matching "rgb").
FILTERS=()
for dir in "${DIRS[@]}"; do
    # Strip src/ prefix and trailing slashes
    mod="${dir#src/}"
    mod="${mod%/}"
    # Replace / with ::
    mod="$(echo "$mod" | sed 's|/|::|g')"
    # Append :: to ensure prefix matching
    mod="${mod}::"
    FILTERS+=("$mod")
done

CARGO_FLAGS="${CARGO_TEST_ARGS:---no-default-features}"

# Build the cargo test command
CMD=(cargo test $CARGO_FLAGS --lib --)

# Add filters
for f in "${FILTERS[@]}"; do
    CMD+=("$f")
done

# Add --skip for integration tests if requested
if $SKIP_INTEGRATION; then
    CMD+=(--skip 'nes::integration_tests')
    CMD+=(--skip 'gb::integration_tests')
    CMD+=(--skip 'gba::integration_tests')
    CMD+=(--skip 'snes::integration_tests')
fi

# Add --list if requested
if $LIST_ONLY; then
    CMD+=(--list)
fi

# Add any passthrough args
if [ ${#EXTRA_ARGS[@]} -gt 0 ]; then
    CMD+=("${EXTRA_ARGS[@]}")
fi

echo "Running: ${CMD[*]}" >&2
exec "${CMD[@]}"
