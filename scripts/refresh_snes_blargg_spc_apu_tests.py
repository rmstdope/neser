"""Optional-local refresher for SNES SPC/APU ROM pass-fail corpus.

This helper is referenced by the SNES manifest entry
`snes-rom-pass-fail-blargg-spc-apu` as the `refresh_command` for the
optional_full_suite variant. It supports a dry-run mode that only reports the
planned destination without performing network I/O so that downstream tooling
and CI can exercise the helper without depending on external services.
"""

from __future__ import annotations

import argparse
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
    dry_run: bool
    fetched_files: list[Path] = field(default_factory=list)


def plan_refresh(dest_root: Path, dry_run: bool) -> RefreshPlan:
    """Return a plan that summarizes the intended refresh action.

    The current implementation only describes the plan and does not fetch
    anything yet. Network fetching is intentionally deferred to a later
    iteration once licensed sources are confirmed.
    """

    return RefreshPlan(dest_root=dest_root, dry_run=dry_run)


def _parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--dest-root",
        type=Path,
        default=DEFAULT_DEST_ROOT,
        help="Destination directory for fetched ROMs.",
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
    plan = plan_refresh(dest_root=args.dest_root, dry_run=args.dry_run)

    if plan.dry_run:
        print(
            f"dry-run: would refresh SNES SPC/APU corpus into {plan.dest_root}",
        )
        return 0

    print(
        "error: network fetching is not yet implemented; "
        "rerun with --dry-run to preview the planned destination.",
    )
    return 1


def main() -> int:
    import sys

    return run_refresh(sys.argv[1:])


if __name__ == "__main__":
    raise SystemExit(main())
