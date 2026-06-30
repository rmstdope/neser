"""Compute tree_sha256 integrity metadata for a committed SNES ROM asset directory.

The committed `rom_pass_fail` / ROM-binary SNES assets are vendored binaries
rather than generated artifacts, so there is no per-suite refresh script that
recomputes their manifest integrity. This helper reproduces the same
`tree_sha256` record format used by the ProcessorTests refresh scripts: a
newline-joined list of ``"<sha256>  <filename>"`` records (sorted by filename),
hashed as UTF-8, plus the file count and total size.

Usage::

    python -m scripts.compute_snes_rom_asset_integrity \
        roms/snes/automated_tests/rom_pass_fail/blargg_spc_apu/v1
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any

ROM_EXTENSIONS = (".smc", ".sfc")


def compute_integrity(root: Path) -> dict[str, Any]:
    """Return tree_sha256 integrity metadata for the ROM files under ``root``."""

    files = sorted(
        path
        for path in root.iterdir()
        if path.is_file() and path.suffix.lower() in ROM_EXTENSIONS
    )

    records: list[str] = []
    total_size_bytes = 0
    for path in files:
        data = path.read_bytes()
        total_size_bytes += len(data)
        records.append(f"{hashlib.sha256(data).hexdigest()}  {path.name}")

    tree_payload = "\n".join(records) + "\n"
    return {
        "kind": "tree_sha256",
        "sha256": hashlib.sha256(tree_payload.encode("utf-8")).hexdigest(),
        "file_count": len(files),
        "total_size_bytes": total_size_bytes,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path, help="ROM asset directory to hash")
    args = parser.parse_args()

    if not args.root.is_dir():
        print(f"error: not a directory: {args.root}")
        return 1

    print(json.dumps(compute_integrity(args.root), indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
