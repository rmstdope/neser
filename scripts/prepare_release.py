"""Prepare a NESER release: next version, Cargo versions, scroll text and notes.

The release skill (.claude/skills/release/SKILL.md) decides what a release says; this script
makes the edits that must be exact. ``--print-version`` answers "what would the next version
be"; ``--highlights`` and ``--notes`` apply the release to the tree:

    python -m scripts.prepare_release --kind minor --print-version
    python -m scripts.prepare_release --kind minor --highlights "..." --notes /tmp/notes.md

Applying a release changes ``Cargo.toml`` and ``Cargo.lock`` (the ``neser`` package only), the
``SCROLLER_TEXT`` sentence in ``web/src/app.ts``, and copies the notes to
``docs/releases/v<version>.md``, which the release workflow uses as the GitHub Release body.
"""

import argparse
import datetime
import enum
import re
import shutil
from pathlib import Path


class ReleaseKind(enum.Enum):
    """Which digit of the version a release steps."""

    MAINTENANCE = "maintenance"
    MINOR = "minor"
    MAJOR = "major"


_VERSION_RE = re.compile(r"^(\d+)\.(\d+)\.(\d+)$")
_CARGO_TOML_VERSION_RE = re.compile(r'(?m)^(\[package\]\n(?:(?!\[).*\n)*?version = ")(\d+\.\d+\.\d+)(")')
_CARGO_LOCK_VERSION_RE = re.compile(r'(?m)^(name = "neser"\nversion = ")(\d+\.\d+\.\d+)(")')
_SCROLLER_RE = re.compile(r'(?m)^(const SCROLLER_TEXT = ")([^"\n]*)(";)')


def bump_version(version: str, kind: ReleaseKind) -> str:
    """Return the version after a release of ``kind``.

    Maintenance steps the third digit; minor steps the second and resets the third; major steps
    the first and clears the other two.
    """
    match = _VERSION_RE.match(version)
    if not match:
        raise ValueError(f"version must look like 1.2.3, got {version!r}")
    major, minor, patch = (int(part) for part in match.groups())
    if kind is ReleaseKind.MAINTENANCE:
        return f"{major}.{minor}.{patch + 1}"
    if kind is ReleaseKind.MINOR:
        return f"{major}.{minor + 1}.0"
    return f"{major + 1}.0.0"


def format_release_date(day: datetime.date) -> str:
    """``September 7, 2026``: the shape the scroll text has always used."""
    return f"{day.strftime('%B')} {day.day}, {day.year}"


def render_scroller_sentence(day: datetime.date, version: str, highlights: str) -> str:
    """The one sentence the idle scroller shows: date, version, and the release's highlights."""
    text = highlights.strip()
    if '"' in text:
        raise ValueError("highlights cannot contain a double quote: they are written into a TypeScript string literal")
    if not text:
        raise ValueError("highlights must not be empty")
    return f"{format_release_date(day)}: Version {version} - {text}"


def _replace_once(pattern: re.Pattern[str], text: str, version: str, what: str) -> str:
    updated, count = pattern.subn(lambda m: f"{m.group(1)}{version}{m.group(3)}", text)
    if count == 0:
        raise ValueError(f"could not find {what}")
    if count > 1:
        raise ValueError(f"found {what} {count} times; expected exactly one")
    return updated


def update_cargo_toml(text: str, version: str) -> str:
    """Set the ``[package]`` version and nothing else."""
    return _replace_once(_CARGO_TOML_VERSION_RE, text, version, "the [package] version in Cargo.toml")


def update_cargo_lock(text: str, version: str) -> str:
    """Set the ``neser`` package's version in the lockfile and nothing else."""
    return _replace_once(_CARGO_LOCK_VERSION_RE, text, version, 'the "neser" package in Cargo.lock')


def update_scroller_text(text: str, sentence: str) -> str:
    """Replace the whole ``SCROLLER_TEXT`` sentence in ``web/src/app.ts``."""
    return _replace_once(_SCROLLER_RE, text, sentence, "SCROLLER_TEXT in web/src/app.ts")


def current_version(repo_root: Path) -> str:
    """The version ``Cargo.toml`` carries now."""
    match = _CARGO_TOML_VERSION_RE.search((repo_root / "Cargo.toml").read_text(encoding="utf-8"))
    if not match:
        raise ValueError("could not find the [package] version in Cargo.toml")
    return match.group(2)


def apply_release(
    repo_root: Path,
    kind: ReleaseKind,
    *,
    highlights: str,
    notes_path: Path,
    today: datetime.date | None = None,
) -> str:
    """Write the release into the tree and return the new version.

    Every edit is computed before any file is written, so a missing anchor or an existing notes
    file leaves the tree untouched.
    """
    day = today or datetime.date.today()
    version = bump_version(current_version(repo_root), kind)
    cargo_toml = repo_root / "Cargo.toml"
    cargo_lock = repo_root / "Cargo.lock"
    app_ts = repo_root / "web" / "src" / "app.ts"
    notes_target = repo_root / "docs" / "releases" / f"v{version}.md"

    edits = [
        (cargo_toml, update_cargo_toml(cargo_toml.read_text(encoding="utf-8"), version)),
        (cargo_lock, update_cargo_lock(cargo_lock.read_text(encoding="utf-8"), version)),
        (
            app_ts,
            update_scroller_text(
                app_ts.read_text(encoding="utf-8"), render_scroller_sentence(day, version, highlights)
            ),
        ),
    ]
    if notes_target.exists():
        raise FileExistsError(f"{notes_target} already exists; a release's notes are written once")
    if not notes_path.is_file():
        raise FileNotFoundError(f"notes file {notes_path} not found")

    for path, text in edits:
        path.write_text(text, encoding="utf-8")
    notes_target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(notes_path, notes_target)
    return version


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--repo-root", type=Path, default=Path.cwd())
    parser.add_argument("--kind", choices=[kind.value for kind in ReleaseKind], required=True)
    parser.add_argument("--print-version", action="store_true", help="print the next version and change nothing")
    parser.add_argument(
        "--current-version",
        help="bump from this version instead of the tree's Cargo.toml (with --print-version, e.g. main's version)",
    )
    parser.add_argument("--highlights", help="the short list of the most important changes, one sentence or a few")
    parser.add_argument("--notes", type=Path, help="the approved release notes, copied to docs/releases/v<version>.md")
    parser.add_argument("--date", type=datetime.date.fromisoformat, help="release date (default: today)")
    args = parser.parse_args(argv)

    kind = ReleaseKind(args.kind)
    if args.print_version:
        print(bump_version(args.current_version or current_version(args.repo_root), kind))
        return 0
    if args.current_version:
        parser.error("--current-version only goes with --print-version; applying reads the tree")
    if not args.highlights or not args.notes:
        parser.error("--highlights and --notes are required to apply a release (or pass --print-version)")

    version = apply_release(args.repo_root, kind, highlights=args.highlights, notes_path=args.notes, today=args.date)
    print(version)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
