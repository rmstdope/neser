#!/bin/sh
# Makes sure the Chromium Playwright installed can load its system libraries, touching apt only
# when one is really missing (nr-2e2l).
#
# `npx playwright install-deps chromium` goes to the apt mirror every time, and on a slow mirror it
# took up to eleven minutes in CI, although the GitHub runner image already carries every library
# Chromium links against: what it added were extra fonts and package upgrades the suite does not
# need. So this runs `ldd` over the installed Chromium binaries and the shared libraries beside
# them, as Playwright's own host validation does, and runs install-deps only if a library is
# missing or no Chromium is installed.
#
# Environment: PLAYWRIGHT_BROWSERS_PATH (default ~/.cache/ms-playwright), and
# PLAYWRIGHT_DEPS_RETRY_DELAY, the seconds between install-deps attempts (default 15).
set -eu

browsers="${PLAYWRIGHT_BROWSERS_PATH:-$HOME/.cache/ms-playwright}"
delay="${PLAYWRIGHT_DEPS_RETRY_DELAY:-15}"

binaries="$(find "$browsers" -path '*chrom*' -type f \
    \( -name chrome -o -name chrome-headless-shell -o -name '*.so' -o -name '*.so.*' \) \
    2>/dev/null || true)"

reason=""
if [ -z "$binaries" ]; then
    reason="no Chromium is installed under $browsers"
else
    # ldd exits non-zero for a file that is not a dynamic executable; only its "not found" lines
    # matter here.
    missing="$(printf '%s\n' "$binaries" | tr '\n' '\0' | xargs -0 ldd 2>/dev/null |
        grep 'not found' | sort -u || true)"
    if [ -n "$missing" ]; then
        reason="Chromium is missing system libraries:
$missing"
    fi
fi

if [ -z "$reason" ]; then
    echo "Chromium's system libraries are all present; skipping apt."
    exit 0
fi

echo "$reason"
echo "Installing Playwright's system dependencies..."
attempt=1
while :; do
    if npx playwright install-deps chromium; then
        exit 0
    fi
    if [ "$attempt" -ge 3 ]; then
        echo "Playwright install-deps failed $attempt times; giving up." >&2
        exit 1
    fi
    echo "Playwright install-deps attempt $attempt failed (transient network); retrying after ${delay}s..."
    attempt=$((attempt + 1))
    sleep "$delay"
done
