"""Pick a ChromeDriver whose major version matches the installed Chrome.

`wasm-pack test --headless --chrome` uses whatever ChromeDriver it has cached, and when that
driver's major version differs from Chrome's the run dies before any test with an opaque
`http status: 404`. Setting `CHROMEDRIVER` does not help: wasm-pack exports its own path over
it; only its `--chromedriver <path>` flag works. `scripts/gate-full.sh` runs this first and
passes the path it prints.

    python scripts/chromedriver_match.py     # prints the matching driver's path, exit 0
                                             # or one line naming the mismatch, exit 1

Standard library only, and Python 3.9 safe: the gate may run it with the system python3.
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys
from collections.abc import Mapping
from pathlib import Path

Version = tuple[int, ...]

MAC_CHROME_APP = "Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
LINUX_CHROMES = ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser"]
WASM_PACK_CACHES = ["Library/Caches/.wasm-pack", ".cache/.wasm-pack"]
CHROME_FOR_TESTING = "https://googlechromelabs.github.io/chrome-for-testing/"

_VERSION = re.compile(r"\b(\d+(?:\.\d+)+)\b")


def parse_version(text: str) -> Version | None:
    """Return the first dotted version in a `--version` output, or None."""
    match = _VERSION.search(text)
    if match is None:
        return None
    return tuple(int(part) for part in match.group(1).split("."))


def choose_driver(chrome: Version, drivers: list[tuple[Path, Version | None]]) -> Path | None:
    """Return the first driver whose major version equals Chrome's, or None."""
    for path, version in drivers:
        if version is not None and version[0] == chrome[0]:
            return path
    return None


def default_chrome_paths(env: Mapping[str, str], home: Path) -> list[Path]:
    """Return the Chrome binaries wasm-pack could start: the macOS app bundles first, then PATH."""
    paths = [app for app in (Path("/") / MAC_CHROME_APP, home / MAC_CHROME_APP) if app.is_file()]
    for name in LINUX_CHROMES:
        found = shutil.which(name, path=env.get("PATH", ""))
        if found is not None:
            paths.append(Path(found))
    return paths


def default_driver_paths(env: Mapping[str, str], home: Path) -> list[Path]:
    """Return candidate ChromeDrivers: every one on PATH first, then wasm-pack's cached ones."""
    paths: list[Path] = []
    for directory in env.get("PATH", "").split(os.pathsep):
        candidate = Path(directory) / "chromedriver"
        if directory and candidate.is_file() and os.access(candidate, os.X_OK) and candidate not in paths:
            paths.append(candidate)
    for cache in WASM_PACK_CACHES:
        paths.extend(sorted((home / cache).glob("chromedriver-*/chromedriver")))
    return paths


def version_of(binary: Path) -> Version | None:
    """Run `binary --version` and parse it; None when it cannot run or prints no version."""
    try:
        result = subprocess.run([str(binary), "--version"], capture_output=True, text=True, timeout=30, check=False)
    except (OSError, subprocess.TimeoutExpired):
        return None
    return parse_version(result.stdout + result.stderr)


def _dotted(version: Version) -> str:
    return ".".join(str(part) for part in version)


def mismatch_message(chrome: Version, drivers: list[tuple[Path, Version | None]]) -> str:
    """One line naming Chrome's version, every driver found, and the fix."""
    found = ", ".join(f"{_dotted(v) if v is not None else 'unreadable'} at {p}" for p, v in drivers) or "none"
    return (
        f"ChromeDriver does not match Chrome {_dotted(chrome)} (found: {found}); "
        f"put a ChromeDriver {chrome[0]} first on PATH, e.g. from {CHROME_FOR_TESTING}, or update Chrome."
    )


def main(argv: list[str] | None = None) -> int:
    """Print the path of a ChromeDriver matching Chrome and return 0, or one line on stderr and 1."""
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--chrome", type=Path, help="the Chrome binary (default: the one wasm-pack would start)")
    parser.add_argument("--driver", type=Path, action="append", help="a candidate ChromeDriver (repeatable)")
    args = parser.parse_args(argv)

    chromes = [args.chrome] if args.chrome else default_chrome_paths(os.environ, Path.home())
    chrome = next((v for v in map(version_of, chromes) if v is not None), None)
    if chrome is None:
        looked_at = ", ".join(map(str, chromes)) or "nothing"
        print(f"Chrome not found (looked at: {looked_at}); install Chrome to run the wasm tests.", file=sys.stderr)
        return 1

    candidates = args.driver if args.driver else default_driver_paths(os.environ, Path.home())
    drivers = [(path, version_of(path)) for path in candidates if path.is_file()]
    chosen = choose_driver(chrome, drivers)
    if chosen is None:
        print(mismatch_message(chrome, drivers), file=sys.stderr)
        return 1
    print(chosen)
    return 0


if __name__ == "__main__":
    sys.exit(main())
