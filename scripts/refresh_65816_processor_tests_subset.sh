#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST_DIR="$ROOT_DIR/roms/snes/automated_tests/processor_tests/65816/v1"
UPSTREAM_RAW="https://raw.githubusercontent.com/SingleStepTests/ProcessorTests/main/65816/v1"
FILES=("00.e.json" "00.n.json" "ea.e.json" "ea.n.json")

mkdir -p "$DEST_DIR"

echo "Refreshing pinned 65816 subset into $DEST_DIR"
for file in "${FILES[@]}"; do
  curl -Lsf "$UPSTREAM_RAW/$file" | jq '.[0:32]' > "$DEST_DIR/$file"
  echo "  wrote $file"
done

UPSTREAM_SHA="$(curl -Ls https://api.github.com/repos/SingleStepTests/ProcessorTests/commits/main | jq -r '.sha')"
echo "Upstream commit at refresh time: $UPSTREAM_SHA"

echo "Done. Update roms/snes/automated_tests/processor_tests/65816/README.md with the new commit SHA if needed."
