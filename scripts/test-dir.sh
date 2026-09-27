#!/usr/bin/env bash
#
# Run Rust tests for specific source directories.
#
# Converts directory paths under src/ to Rust module filters and passes them
# to `cargo test --lib`. Only test execution is filtered; the full crate is
# still compiled. A directory that matches no test fails before anything runs.
#
# Usage:
#   ./scripts/test-dir.sh src/nes/cartridge          # run cartridge tests
#   ./scripts/test-dir.sh src/gb src/platform         # run gb + platform tests
#   ./scripts/test-dir.sh src/nes --skip-integration  # nes unit tests only
#
# Options:
#   --skip-integration   Exclude every console's integration_tests module
#   --list               List matching tests without running them
#   --print-nextest-skip-expr
#                        Print the cargo-nextest filter that excludes the same
#                        modules as --skip-integration, and exit (CI uses it)
#   --                   Pass remaining args directly to cargo test
#
# Environment:
#   CARGO_TEST_ARGS      Extra arguments passed to cargo test (default: --all-features, as CI)

set -euo pipefail

# rustup's cargo proxy exports RUSTUP_TOOLCHAIN to every process it starts, so a shell opened from
# anything launched with `cargo run` (the Cerebro fleet view, for one) inherits `stable`, which
# beats rust-toolchain.toml. Unset it so this runs on the pinned toolchain, as CI does.
unset RUSTUP_TOOLCHAIN

# The slow ROM-suite modules --skip-integration leaves out. This is the only place the list is
# written: CI's unit-only run takes its nextest filter from --print-nextest-skip-expr, so a new
# core's integration module is added here and nowhere else.
SLOW_MODULES=(
    'nes::integration_tests'
    'gb::integration_tests'
    'gba::integration_tests'
    'snes::integration_tests'
)

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
        --print-nextest-skip-expr)
            EXPR=""
            for module in "${SLOW_MODULES[@]}"; do
                EXPR="${EXPR:+$EXPR | }test($module)"
            done
            echo "not ($EXPR)"
            exit 0
            ;;
        --)
            PASSTHROUGH=true
            ;;
        --help|-h)
            head -23 "$0" | grep '^#' | sed 's/^# \?//'
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
# Trailing :: stops "gb" matching "rgb::", but libtest filters are substrings, so "nes::" also
# matches every "snes::" test (and the zero-match check below can pass on them).
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

CARGO_FLAGS="${CARGO_TEST_ARGS:---all-features}"

# The --skip arguments --skip-integration adds, used by the zero-match check and the run alike.
SKIPS=()
if $SKIP_INTEGRATION; then
    for module in "${SLOW_MODULES[@]}"; do
        SKIPS+=(--skip "$module")
    done
fi

# A directory whose module is not compiled under $CARGO_FLAGS (src/frontends/native under
# --no-default-features, say) would otherwise pass as "0 passed; N filtered out" (nr-5ku). List
# each directory's tests first and stop, naming it, if it has none. The list reuses the build the
# run needs, so it costs only libtest's listing. Passthrough args stay out of it: one like
# `--format json` changes what --list prints. The output is captured before it is counted, so a
# build that fails stops here under `set -e` with cargo's own error, not as "matches no test".
for i in "${!DIRS[@]}"; do
    LIST_CMD=(cargo test $CARGO_FLAGS --lib -- "${FILTERS[$i]}")
    if [ ${#SKIPS[@]} -gt 0 ]; then
        LIST_CMD+=("${SKIPS[@]}")
    fi
    LIST_CMD+=(--list)
    LISTED=$("${LIST_CMD[@]}")
    COUNT=$(printf '%s\n' "$LISTED" | grep -c ': test$' || true)
    if [ "$COUNT" -eq 0 ]; then
        echo "test-dir: ${DIRS[$i]} matches no test under 'cargo test $CARGO_FLAGS --lib'" >&2
        exit 1
    fi
done

# Build the cargo test command
CMD=(cargo test $CARGO_FLAGS --lib --)

# Add filters
for f in "${FILTERS[@]}"; do
    CMD+=("$f")
done

# Add --skip for integration tests if requested
if [ ${#SKIPS[@]} -gt 0 ]; then
    CMD+=("${SKIPS[@]}")
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
