#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST_DIR="$ROOT_DIR/roms/snes/automated_tests/processor_tests/spc700/full/v1"
UPSTREAM_API="https://api.github.com/repos/SingleStepTests/ProcessorTests/contents/spc700/v1"
UPSTREAM_RAW="https://raw.githubusercontent.com/SingleStepTests/ProcessorTests/main/spc700/v1"

mkdir -p "$DEST_DIR"

echo "Refreshing full SPC700 vector corpus into local cache $DEST_DIR"
expected_list="$(mktemp)"
api_response="$(mktemp)"

AUTH_TOKEN="${GITHUB_TOKEN:-${GH_TOKEN:-}}"

if [ -n "$AUTH_TOKEN" ]; then
  http_code="$(curl -sS -L -H "Authorization: Bearer ${AUTH_TOKEN}" -w "%{http_code}" -o "$api_response" "$UPSTREAM_API")"
else
  http_code="$(curl -sS -L -w "%{http_code}" -o "$api_response" "$UPSTREAM_API")"
fi
if [ "$http_code" != "200" ]; then
  message="$(jq -r '.message // empty' "$api_response" 2>/dev/null || true)"
  if [ -n "$message" ]; then
    echo "Failed to fetch SPC700 file list from GitHub API (HTTP $http_code): $message" >&2
  else
    echo "Failed to fetch SPC700 file list from GitHub API (HTTP $http_code)." >&2
  fi
  echo "Tip: export GITHUB_TOKEN (or GH_TOKEN) and retry to avoid anonymous API rate limits." >&2
  rm -f "$expected_list" "$api_response"
  exit 1
fi

if ! jq -e 'type == "array"' "$api_response" >/dev/null; then
  message="$(jq -r '.message // empty' "$api_response" 2>/dev/null || true)"
  if [ -n "$message" ]; then
    echo "Unexpected GitHub API response while fetching SPC700 corpus list: $message" >&2
  else
    echo "Unexpected GitHub API response while fetching SPC700 corpus list." >&2
  fi
  rm -f "$expected_list" "$api_response"
  exit 1
fi

jq -r '.[].name' "$api_response" | sort > "$expected_list"
rm -f "$api_response"

if [ ! -s "$expected_list" ]; then
  echo "No SPC700 files were returned by the upstream API." >&2
  rm -f "$expected_list"
  exit 1
fi

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

if [ -n "$AUTH_TOKEN" ]; then
  UPSTREAM_SHA="$(curl -sS -L -H "Authorization: Bearer ${AUTH_TOKEN}" https://api.github.com/repos/SingleStepTests/ProcessorTests/commits/main | jq -r '.sha // "unknown"')"
else
  UPSTREAM_SHA="$(curl -sS -L https://api.github.com/repos/SingleStepTests/ProcessorTests/commits/main | jq -r '.sha // "unknown"')"
fi
echo "Upstream commit at refresh time: $UPSTREAM_SHA"
echo "Downloaded files: $downloaded_count/$expected_count"

rm -f "$expected_list"

echo "Done. Full vectors are downloaded locally and are intentionally not tracked by git."
