"""Build verified release archives for neser."""

import argparse
import re
import tarfile
import zipfile
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class ReleasePackageConfig:
    """Inputs for building one platform release archive."""

    repo_root: Path
    binary_path: Path
    output_dir: Path
    target: str
    archive_format: str
    binary_name: str = "neser"


def collect_shader_dependencies(repo_root: Path) -> set[Path]:
    """Return shader files required by configured shader presets."""

    repo_root = repo_root.resolve()
    shader_paths = _read_shader_preset_paths(repo_root / "src/platform/shaders.rs")
    dependencies: set[Path] = set()
    processing: set[Path] = set()
    for shader_path in shader_paths:
        _collect_shader_file(
            repo_root, repo_root / shader_path, dependencies, processing
        )
    return dependencies


def create_release_archive(config: ReleasePackageConfig) -> Path:
    """Create a release archive and return its path."""

    if config.archive_format not in {"tar.gz", "zip"}:
        raise ValueError(f"unsupported archive format: {config.archive_format}")

    repo_root = config.repo_root.resolve()
    config.output_dir.mkdir(parents=True, exist_ok=True)
    extension = "tar.gz" if config.archive_format == "tar.gz" else "zip"
    archive_path = config.output_dir / f"neser-{config.target}.{extension}"

    if config.archive_format == "tar.gz":
        with tarfile.open(archive_path, "w:gz") as archive:
            for source_path, archive_path_in_package in _package_files(
                config, repo_root
            ):
                _add_file_to_tar(archive, source_path, archive_path_in_package)
    else:
        with zipfile.ZipFile(archive_path, "w") as archive:
            for source_path, archive_path_in_package in _package_files(
                config, repo_root
            ):
                archive.write(source_path, archive_path_in_package.as_posix())

    return archive_path


def main(argv: list[str] | None = None) -> int:
    """Run the release packager CLI."""

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=Path.cwd())
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--target", required=True)
    parser.add_argument("--format", choices=("tar.gz", "zip"), required=True)
    parser.add_argument("--binary-name", default="neser")
    args = parser.parse_args(argv)

    create_release_archive(
        ReleasePackageConfig(
            repo_root=args.repo_root,
            binary_path=args.binary,
            output_dir=args.output_dir,
            target=args.target,
            archive_format=args.format,
            binary_name=args.binary_name,
        )
    )
    return 0


_PRESET_PATH_RE = re.compile(r'\(\s*"[^"]+"\s*,\s*"([^"]+)"\s*,?\s*\)', re.MULTILINE)
_REFERENCE_RE = re.compile(r'^\s*#reference\s+"([^"]+)"')
_SHADER_RE = re.compile(r"^\s*shader\d+\s*=\s*(.+)$")
_INCLUDE_RE = re.compile(r'^\s*#include\s+"([^"]+)"')
_LUT_QUOTED_RE = re.compile(r'"([^"]+\.(?:png|jpg|jpeg))"', re.IGNORECASE)
_LUT_BARE_RE = re.compile(r"=\s*([^\s]+\.(?:png|jpg|jpeg))", re.IGNORECASE)


def _read_shader_preset_paths(shaders_rs_path: Path) -> list[Path]:
    text = shaders_rs_path.read_text(encoding="utf-8")
    return [Path(match.group(1)) for match in _PRESET_PATH_RE.finditer(text)]


def _collect_shader_file(
    repo_root: Path,
    path: Path,
    dependencies: set[Path],
    processing: set[Path],
) -> None:
    path = path.resolve()
    if path in processing:
        return
    if not path.exists():
        raise FileNotFoundError(path)

    relative_path = path.relative_to(repo_root)
    if relative_path in dependencies:
        return

    processing.add(path)
    try:
        dependencies.add(relative_path)

        suffix = path.suffix.lower()
        if suffix in {".slangp", ".params"}:
            _collect_preset_dependencies(repo_root, path, dependencies, processing)
        elif suffix in {".slang", ".inc", ".h"}:
            _collect_include_dependencies(repo_root, path, dependencies, processing)
    finally:
        processing.remove(path)


def _collect_preset_dependencies(
    repo_root: Path,
    path: Path,
    dependencies: set[Path],
    processing: set[Path],
) -> None:
    for line in path.read_text(encoding="utf-8", errors="ignore").splitlines():
        reference_match = _REFERENCE_RE.match(line)
        if reference_match:
            _collect_relative_shader_file(
                repo_root, path, reference_match.group(1), dependencies, processing
            )
            continue

        if path.suffix.lower() == ".slangp":
            shader_match = _SHADER_RE.match(line)
            if shader_match:
                _collect_relative_shader_file(
                    repo_root,
                    path,
                    _clean_path(shader_match.group(1)),
                    dependencies,
                    processing,
                )
                continue

        for lut_path in _LUT_QUOTED_RE.findall(line):
            _collect_existing_lut(repo_root, path, lut_path, dependencies, processing)
        bare_lut_match = _LUT_BARE_RE.search(line)
        if bare_lut_match:
            _collect_existing_lut(
                repo_root,
                path,
                bare_lut_match.group(1),
                dependencies,
                processing,
            )


def _collect_include_dependencies(
    repo_root: Path,
    path: Path,
    dependencies: set[Path],
    processing: set[Path],
) -> None:
    for line in path.read_text(encoding="utf-8", errors="ignore").splitlines():
        include_match = _INCLUDE_RE.match(line)
        if include_match:
            _collect_relative_shader_file(
                repo_root, path, include_match.group(1), dependencies, processing
            )


def _collect_relative_shader_file(
    repo_root: Path,
    base_file: Path,
    relative_path: str,
    dependencies: set[Path],
    processing: set[Path],
) -> None:
    _collect_shader_file(
        repo_root,
        base_file.parent / _clean_path(relative_path),
        dependencies,
        processing,
    )


def _collect_existing_lut(
    repo_root: Path,
    base_file: Path,
    relative_path: str,
    dependencies: set[Path],
    processing: set[Path],
) -> None:
    lut_path = base_file.parent / _clean_path(relative_path)
    if lut_path.exists():
        _collect_shader_file(repo_root, lut_path, dependencies, processing)


def _clean_path(path: str) -> str:
    path = path.strip()
    if (path.startswith('"') and path.endswith('"')) or (
        path.startswith("'") and path.endswith("'")
    ):
        return path[1:-1]
    return path


def _package_files(
    config: ReleasePackageConfig, repo_root: Path
) -> list[tuple[Path, Path]]:
    package_files = [(config.binary_path, Path("neser") / config.binary_name)]
    package_files.extend(
        _tree_files(repo_root / "assets/fonts", Path("neser/assets/fonts"))
    )
    for required_file in (
        "gamecontrollerdb.txt",
        "neser.conf.example",
        "README.md",
        "README-NES.md",
        "README-GB.md",
        "README-GBA.md",
        "LICENSE",
    ):
        package_files.append((repo_root / required_file, Path("neser") / required_file))

    for shader_dependency in sorted(collect_shader_dependencies(repo_root)):
        package_files.append(
            (repo_root / shader_dependency, Path("neser") / shader_dependency)
        )

    return package_files


def _tree_files(source_dir: Path, archive_dir: Path) -> list[tuple[Path, Path]]:
    if not source_dir.is_dir():
        raise FileNotFoundError(source_dir)

    return [
        (path, archive_dir / path.relative_to(source_dir))
        for path in sorted(source_dir.rglob("*"))
        if path.is_file()
    ]


def _add_file_to_tar(
    archive: tarfile.TarFile, source_path: Path, archive_path: Path
) -> None:
    archive.add(source_path, arcname=archive_path.as_posix(), recursive=False)


if __name__ == "__main__":
    raise SystemExit(main())
