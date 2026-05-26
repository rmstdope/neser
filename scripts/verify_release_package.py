"""Verify neser release archive contents and smoke-test packaged binaries."""

import argparse
import stat
import subprocess
import tarfile
import tempfile
import zipfile
from collections.abc import Iterable
from dataclasses import dataclass
from pathlib import Path


class ReleasePackageVerificationError(Exception):
    """Raised when a release package fails verification."""


@dataclass(frozen=True)
class VerificationConfig:
    """Inputs for verifying one release archive."""

    archive_path: Path
    binary_name: str
    smoke_command: list[str] | None = None
    require_unix_executable: bool = False


def verify_release_package(config: VerificationConfig) -> None:
    """Verify archive contents and optional smoke command."""

    required_paths = _required_paths(config.binary_name)
    with tempfile.TemporaryDirectory() as temp_dir_str:
        extract_root = Path(temp_dir_str)
        if _is_tar_gz(config.archive_path):
            _verify_tar_gz(config, required_paths, extract_root)
        elif config.archive_path.suffix == ".zip":
            _verify_zip(config, required_paths, extract_root)
        else:
            raise ReleasePackageVerificationError(
                f"unsupported archive format: {config.archive_path}"
            )

        if config.smoke_command is not None:
            _run_smoke_command(config.smoke_command, extract_root / "neser")


def main(argv: list[str] | None = None) -> int:
    """Run the release package verifier CLI."""

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive_path", type=Path)
    parser.add_argument("--binary-name", required=True)
    parser.add_argument("--require-unix-executable", action="store_true")
    parser.add_argument("--smoke-command", nargs=argparse.REMAINDER)
    args = parser.parse_args(argv)

    verify_release_package(
        VerificationConfig(
            archive_path=args.archive_path,
            binary_name=args.binary_name,
            smoke_command=args.smoke_command,
            require_unix_executable=args.require_unix_executable,
        )
    )
    return 0


def _required_paths(binary_name: str) -> set[str]:
    return {
        f"neser/{binary_name}",
        "neser/assets/fonts/NunitoSans-Regular.ttf",
        "neser/gamecontrollerdb.txt",
        "neser/neser.conf.example",
        "neser/README.md",
        "neser/README-NES.md",
        "neser/README-GB.md",
        "neser/README-GBA.md",
        "neser/LICENSE",
        "neser/shaders/stock.slangp",
        "neser/shaders/stock.slang",
    }


def _is_tar_gz(path: Path) -> bool:
    return path.name.endswith(".tar.gz")


def _verify_tar_gz(
    config: VerificationConfig,
    required_paths: set[str],
    extract_root: Path,
) -> None:
    with tarfile.open(config.archive_path, "r:gz") as archive:
        members = {member.name: member for member in archive.getmembers()}
        _verify_required_paths(required_paths, set(members))

        if config.require_unix_executable:
            binary_path = f"neser/{config.binary_name}"
            if not members[binary_path].mode & stat.S_IXUSR:
                raise ReleasePackageVerificationError(f"not executable: {binary_path}")

        _verify_archive_paths_are_safe(set(members))
        _verify_tar_members_are_extractable(members.values())
        archive.extractall(extract_root)


def _verify_zip(
    config: VerificationConfig,
    required_paths: set[str],
    extract_root: Path,
) -> None:
    with zipfile.ZipFile(config.archive_path) as archive:
        names = set(archive.namelist())
        _verify_required_paths(required_paths, names)
        _verify_archive_paths_are_safe(names)
        archive.extractall(extract_root)


def _verify_required_paths(required_paths: set[str], names: set[str]) -> None:
    missing_paths = sorted(required_paths - names)
    if missing_paths:
        raise ReleasePackageVerificationError(
            f"missing required path: {missing_paths[0]}"
        )


def _verify_archive_paths_are_safe(names: set[str]) -> None:
    for name in names:
        path = Path(name)
        if path.is_absolute() or ".." in path.parts:
            raise ReleasePackageVerificationError(f"unsafe archive path: {name}")


def _verify_tar_members_are_extractable(
    members: Iterable[tarfile.TarInfo],
) -> None:
    for member in members:
        if member.issym() or member.islnk():
            raise ReleasePackageVerificationError(f"unsupported tar link: {member.name}")


def _run_smoke_command(smoke_command: list[str], cwd: Path) -> None:
    result = subprocess.run(
        smoke_command,
        cwd=cwd,
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        output = (result.stderr or result.stdout).strip()
        raise ReleasePackageVerificationError(
            f"smoke command failed with exit code {result.returncode}: {output}"
        )


if __name__ == "__main__":
    raise SystemExit(main())
