#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST_DIR="$ROOT_DIR/roms/snes/automated_tests/processor_tests/spc700/full/v1"
UPSTREAM_API="https://api.github.com/repos/SingleStepTests/ProcessorTests/contents/spc700/v1"
UPSTREAM_RAW="https://raw.githubusercontent.com/SingleStepTests/ProcessorTests/main/spc700/v1"

mkdir -p "$DEST_DIR"

echo "Refreshing full SPC700 vector corpus into local cache $DEST_DIR"
expected_list="$(mktemp)"
curl -Lsf "$UPSTREAM_API" | jq -r '.[].name' | sort > "$expected_list"

while IFS= read -r file; do
  [ -n "$file" ] || continue
  curl -Lsf "$UPSTREAM_RAW/$file" > "$DEST_DIR/$file"
  echo "  wrote $file"
done < "$expected_list"

missing_list="$(mktemp)"
while IFS= read -r file; do
  [ -n "$file" ] || continue
  if [ ! -s "$DEST_DIR/$file" ]; then
    echo "$file" >> "$missing_list"
  fi
done < "$expected_list"

expected_count="$(wc -l < "$expected_list" | tr -d '[:space:]')"
downloaded_count="$(find "$DEST_DIR" -maxdepth 1 -type f -name '*.json' | wc -l | tr -d '[:space:]')"

if [ -s "$missing_list" ]; then
  echo "Missing downloaded files:" >&2
  cat "$missing_list" >&2
  rm -f "$expected_list" "$missing_list"
  exit 1
fi

rm -f "$missing_list"

UPSTREAM_SHA="$(curl -Ls https://api.github.com/repos/SingleStepTests/ProcessorTests/commits/main | jq -r '.sha')"
echo "Upstream commit at refresh time: $UPSTREAM_SHA"
echo "Downloaded files: $downloaded_count/$expected_count"

rm -f "$expected_list"

echo "Done. Full vectors are downloaded locally and are intentionally not tracked by git."
