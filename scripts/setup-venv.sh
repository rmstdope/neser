#!/usr/bin/env bash
# Build the project's .venv with the Python tooling the gate needs, exactly as CI's Python job
# installs it: pip >= 25.1 (for PEP 735 `--group`) and the `test` and `dev` dependency groups of
# scripts/pyproject.toml. An existing .venv is reused and brought up to the pins.
#
#   ./scripts/setup-venv.sh                    uses python3
#   PYTHON=python3.14 ./scripts/setup-venv.sh  uses the interpreter CI pins
#
# .cerebro/project.conf runs this as part of `install_shell', so every prepared worktree has it.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

if [[ ! -x .venv/bin/python ]]; then
  "${PYTHON:-python3}" -m venv .venv
fi
.venv/bin/python -m pip install --upgrade "pip>=25.1"
.venv/bin/python -m pip install --group scripts/pyproject.toml:test --group scripts/pyproject.toml:dev
