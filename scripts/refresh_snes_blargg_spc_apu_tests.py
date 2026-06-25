"""Optional-local refresher for SNES SPC/APU ROM pass-fail corpus.

This helper is referenced by the SNES manifest entry
`snes-rom-pass-fail-blargg-spc-apu` as the `refresh_command` for the
optional_full_suite variant. It supports a dry-run mode that only reports the
planned destination without performing network I/O so that downstream tooling
and CI can exercise the helper without depending on external services.
"""

from __future__ import annotations

import argparse
import shutil
from dataclasses import dataclass, field
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_DEST_ROOT = (
    REPO_ROOT / "roms/snes/automated_tests/rom_pass_fail/blargg_spc_apu/full/v1"
)


@dataclass
class RefreshPlan:
    """Description of a planned ROM refresh action."""

    dest_root: Path
    source_dir: Path | None
    dry_run: bool
    fetched_files: list[Path] = field(default_factory=list)


def _discover_rom_files(source_dir: Path) -> list[Path]:
    files = [
        path
        for path in source_dir.iterdir()
        if path.is_file() and path.suffix.lower() in {".sfc", ".smc"}
    ]
    files.sort()
    return files


def plan_refresh(dest_root: Path, source_dir: Path | None, dry_run: bool) -> RefreshPlan:
    """Return a plan that summarizes the intended refresh action.

    This helper performs local intake only: it discovers ROMs from source_dir
    (when provided) and plans copies into dest_root. Network fetching remains
    intentionally out-of-scope until licensed sources are confirmed.
    """

    fetched_files: list[Path] = []
    if source_dir is not None:
        if not source_dir.exists() or not source_dir.is_dir():
            raise ValueError(f"source_dir does not exist or is not a directory: {source_dir}")
        fetched_files = _discover_rom_files(source_dir)

    return RefreshPlan(
        dest_root=dest_root,
        source_dir=source_dir,
        dry_run=dry_run,
        fetched_files=fetched_files,
    )


def _parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--dest-root",
        type=Path,
        default=DEFAULT_DEST_ROOT,
        help="Destination directory for fetched ROMs.",
    )
    parser.add_argument(
        "--source-dir",
        type=Path,
        default=None,
        help="Local source directory containing .sfc/.smc files to copy.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Describe the planned action without writing any files.",
    )
    return parser.parse_args(argv)


def run_refresh(argv: list[str]) -> int:
    """Entry point used by CLI and tests. Returns a process exit code."""

    args = _parse_args(argv)
    try:
        plan = plan_refresh(
            dest_root=args.dest_root,
            source_dir=args.source_dir,
            dry_run=args.dry_run,
        )
    except ValueError as err:
        print(f"error: {err}")
        return 1

    if plan.dry_run:
        print(f"dry-run: would refresh SNES SPC/APU corpus into {plan.dest_root}")
        if plan.source_dir is None:
            print("dry-run: no source directory provided; no files would be copied")
        else:
            print(
                f"dry-run: discovered {len(plan.fetched_files)} ROM file(s) in {plan.source_dir}",
            )
        return 0

    if plan.source_dir is None:
        print(
            "error: --source-dir is required for non-dry-run local intake",
        )
        return 1

    plan.dest_root.mkdir(parents=True, exist_ok=True)
    copied = 0
    for rom_path in plan.fetched_files:
        target = plan.dest_root / rom_path.name
        shutil.copy2(rom_path, target)
        copied += 1

    print(
        f"copied {copied} ROM file(s) from {plan.source_dir} into {plan.dest_root}",
    )
    return 0


def main() -> int:
    import sys

    return run_refresh(sys.argv[1:])


if __name__ == "__main__":
    raise SystemExit(main())
