#!/usr/bin/env bash
# The full pre-merge gate: every checkpoint CLAUDE.md requires before anything lands on main.
#
# One definition shared by the Cerebro fleet (.cerebro/project.conf gate_full) and by people
# running it by hand. CI runs the same commands job by job in .github/workflows/ci.yml.
#
#   ./scripts/gate-full.sh            run everything, stop at the first failure
#   ./scripts/gate-full.sh --fast     the fast subset only (fmt, host clippy, unit tests)
#
# The Python steps use the project's .venv directly rather than `source .venv/bin/activate`,
# because the activate script hardcodes the path the venv was created at.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

# rustup's cargo proxy exports RUSTUP_TOOLCHAIN to every process it starts, so a shell opened from
# anything launched with `cargo run` (the Cerebro fleet view, for one) inherits `stable`, which
# beats rust-toolchain.toml. Unset it so this runs on the pinned toolchain, as CI does.
unset RUSTUP_TOOLCHAIN

fast_only=0
[[ "${1:-}" == "--fast" ]] && fast_only=1

step() {
  echo
  echo "==> $*"
  "$@"
}

py() {
  if [[ -x .venv/bin/python ]]; then .venv/bin/python "$@"; else python3 "$@"; fi
}

step cargo fmt --all -- --check
step cargo clippy --all-targets --all-features -- -D warnings
# Every unit test; the ROM-driven integration_tests modules (97% of runtime) wait for the full gate.
step cargo test --no-default-features --lib -- --skip integration_tests

if [[ $fast_only -eq 1 ]]; then
  echo
  echo "fast gate passed"
  exit 0
fi

step cargo test --no-default-features --lib
step cargo test --doc
step cargo clippy --target wasm32-unknown-unknown --no-default-features --features wasm --all-targets -- -D warnings
step cargo clippy --no-default-features --features frontend --all-targets -- -D warnings
step wasm-pack test --headless --chrome --no-default-features --features wasm
step py -m unittest discover -s scripts -t . -p "test_*.py"
step py -m ruff check scripts
step py -m ruff format --check scripts
step py -m mypy --config-file scripts/pyproject.toml scripts
step npm test

echo
echo "full gate passed"
