"""Unit tests for release package verification."""

import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path

from scripts.verify_release_package import (
    ReleasePackageVerificationError,
    VerificationConfig,
    main,
    verify_release_package,
)


def write_text(path: Path, text: str) -> None:
    """Write UTF-8 text, creating parent directories first."""

    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def create_package_tree(root: Path, *, binary_name: str = "neser") -> Path:
    """Create a minimal extracted release package tree."""

    package_root = root / "neser"
    write_text(package_root / "assets/fonts/NunitoSans-Regular.ttf", "font\n")
    write_text(package_root / "gamecontrollerdb.txt", "controller mappings\n")
    write_text(package_root / "neser.conf.example", "# config\n")
    write_text(package_root / "README.md", "# neser\n")
    write_text(package_root / "README-NES.md", "# nes\n")
    write_text(package_root / "README-GB.md", "# gb\n")
    write_text(package_root / "README-GBA.md", "# gba\n")
    write_text(package_root / "LICENSE", "license\n")
    write_text(package_root / "shaders/stock.slangp", "shader0 = stock.slang\n")
    write_text(package_root / "shaders/stock.slang", "void main() {}\n")
    binary_path = package_root / binary_name
    write_text(binary_path, "#!/bin/sh\nprintf 'neser 1.0.0\\n'\n")
    binary_path.chmod(0o755)
    return package_root


def create_tar_gz(package_root: Path, archive_path: Path) -> None:
    """Create a tar.gz archive from an extracted package root."""

    with tarfile.open(archive_path, "w:gz") as archive:
        archive.add(package_root, arcname="neser")


def create_zip(package_root: Path, archive_path: Path) -> None:
    """Create a zip archive from an extracted package root."""

    with zipfile.ZipFile(archive_path, "w") as archive:
        for path in sorted(package_root.rglob("*")):
            if path.is_file():
                archive.write(path, Path("neser") / path.relative_to(package_root))


class VerifyReleasePackageTests(unittest.TestCase):
    """Behavior tests for release archive verification."""

    def test_verify_tar_gz_package_accepts_required_layout_and_smoke_command(
        self,
    ) -> None:
        """A complete Unix package passes manifest, permission, and smoke checks."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_dir = Path(temp_dir_str)
            package_root = create_package_tree(temp_dir / "src")
            archive_path = temp_dir / "neser-linux-x86_64.tar.gz"
            create_tar_gz(package_root, archive_path)

            verify_release_package(
                VerificationConfig(
                    archive_path=archive_path,
                    binary_name="neser",
                    smoke_command=["./neser", "--version"],
                    require_unix_executable=True,
                )
            )

    def test_verify_zip_package_accepts_required_windows_layout(self) -> None:
        """A complete Windows zip package passes manifest checks."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_dir = Path(temp_dir_str)
            package_root = create_package_tree(
                temp_dir / "src", binary_name="neser.exe"
            )
            archive_path = temp_dir / "neser-windows-x86_64.zip"
            create_zip(package_root, archive_path)

            verify_release_package(
                VerificationConfig(
                    archive_path=archive_path,
                    binary_name="neser.exe",
                )
            )

    def test_verify_package_reports_missing_required_resource(self) -> None:
        """Missing runtime resources fail with the missing archive path."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_dir = Path(temp_dir_str)
            package_root = create_package_tree(temp_dir / "src")
            (package_root / "gamecontrollerdb.txt").unlink()
            archive_path = temp_dir / "neser-linux-x86_64.tar.gz"
            create_tar_gz(package_root, archive_path)

            with self.assertRaisesRegex(
                ReleasePackageVerificationError, "neser/gamecontrollerdb.txt"
            ):
                verify_release_package(
                    VerificationConfig(
                        archive_path=archive_path,
                        binary_name="neser",
                        require_unix_executable=True,
                    )
                )

    def test_verify_package_reports_missing_system_readme(self) -> None:
        """Release archives must keep system-specific README links valid."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_dir = Path(temp_dir_str)
            package_root = create_package_tree(temp_dir / "src")
            (package_root / "README-NES.md").unlink()
            archive_path = temp_dir / "neser-linux-x86_64.tar.gz"
            create_tar_gz(package_root, archive_path)

            with self.assertRaisesRegex(
                ReleasePackageVerificationError, "neser/README-NES.md"
            ):
                verify_release_package(
                    VerificationConfig(
                        archive_path=archive_path,
                        binary_name="neser",
                    )
                )

    def test_verify_package_requires_default_shader_source(self) -> None:
        """The verifier requires the shader source referenced by stock.slangp."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_dir = Path(temp_dir_str)
            package_root = create_package_tree(temp_dir / "src")
            (package_root / "shaders/stock.slang").unlink()
            archive_path = temp_dir / "neser-linux-x86_64.tar.gz"
            create_tar_gz(package_root, archive_path)

            with self.assertRaisesRegex(
                ReleasePackageVerificationError, "neser/shaders/stock.slang"
            ):
                verify_release_package(
                    VerificationConfig(
                        archive_path=archive_path,
                        binary_name="neser",
                    )
                )

    def test_verify_tar_gz_package_rejects_link_members(self) -> None:
        """Tar packages with symlink members are rejected before extraction."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_dir = Path(temp_dir_str)
            package_root = create_package_tree(temp_dir / "src")
            archive_path = temp_dir / "neser-linux-x86_64.tar.gz"
            with tarfile.open(archive_path, "w:gz") as archive:
                archive.add(package_root, arcname="neser")
                link_info = tarfile.TarInfo("neser/shaders/escape")
                link_info.type = tarfile.SYMTYPE
                link_info.linkname = "../../outside"
                archive.addfile(link_info)

            with self.assertRaisesRegex(
                ReleasePackageVerificationError, "unsupported tar link"
            ):
                verify_release_package(
                    VerificationConfig(
                        archive_path=archive_path,
                        binary_name="neser",
                    )
                )

    def test_verify_package_reports_unsupported_single_suffix_archive(self) -> None:
        """Unsupported archive suffixes fail with a clean verifier error."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            archive_path = Path(temp_dir_str) / "neser-linux-x86_64.tgz"
            archive_path.write_bytes(b"not an archive")

            with self.assertRaisesRegex(
                ReleasePackageVerificationError, "unsupported archive format"
            ):
                verify_release_package(
                    VerificationConfig(
                        archive_path=archive_path,
                        binary_name="neser",
                    )
                )

    def test_main_verifies_archive_from_cli_arguments(self) -> None:
        """CLI arguments verify an archive and run the smoke command."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_dir = Path(temp_dir_str)
            package_root = create_package_tree(temp_dir / "src")
            archive_path = temp_dir / "neser-linux-x86_64.tar.gz"
            create_tar_gz(package_root, archive_path)

            exit_code = main(
                [
                    str(archive_path),
                    "--binary-name",
                    "neser",
                    "--require-unix-executable",
                    "--smoke-command",
                    "./neser",
                    "--version",
                ]
            )

            self.assertEqual(exit_code, 0)


if __name__ == "__main__":
    unittest.main()
