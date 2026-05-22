"""Unit tests for release package assembly."""

import stat
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path

from scripts.package_release import (
    ReleasePackageConfig,
    collect_shader_dependencies,
    create_release_archive,
)


def write_text(path: Path, text: str) -> None:
    """Write UTF-8 text, creating parent directories first."""

    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def write_bytes(path: Path, data: bytes = b"test") -> None:
    """Write bytes, creating parent directories first."""

    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)


class PackageReleaseTests(unittest.TestCase):
    """Behavior tests for release archive creation."""

    def test_collect_shader_dependencies_resolves_configured_preset_graph(self) -> None:
        """Configured shader presets include referenced presets, passes, includes, and LUTs."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            repo_root = Path(temp_dir_str)
            write_text(
                repo_root / "src/platform/shaders.rs",
                """
pub const SHADER_PRESETS: &[(&str, &str)] = &[
    (
        "crt",
        "vendor/slang-shaders/crt/preset.slangp",
    ),
];
""",
            )
            write_text(
                repo_root / "vendor/slang-shaders/crt/preset.slangp",
                '#reference "../common/base.slangp"\n'
                "shader0 = pass.slang\n"
                'textures = "lut.png"\n',
            )
            write_text(
                repo_root / "vendor/slang-shaders/crt/pass.slang",
                '#include "shared.inc"\n',
            )
            write_text(
                repo_root / "vendor/slang-shaders/crt/shared.inc", "#define TEST 1\n"
            )
            write_bytes(repo_root / "vendor/slang-shaders/crt/lut.png")
            write_text(
                repo_root / "vendor/slang-shaders/common/base.slangp",
                "shader0 = base.slang\n",
            )
            write_text(
                repo_root / "vendor/slang-shaders/common/base.slang", "void main() {}\n"
            )

            dependencies = collect_shader_dependencies(repo_root)

            self.assertEqual(
                dependencies,
                {
                    Path("vendor/slang-shaders/crt/preset.slangp"),
                    Path("vendor/slang-shaders/crt/pass.slang"),
                    Path("vendor/slang-shaders/crt/shared.inc"),
                    Path("vendor/slang-shaders/crt/lut.png"),
                    Path("vendor/slang-shaders/common/base.slangp"),
                    Path("vendor/slang-shaders/common/base.slang"),
                },
            )

    def test_create_tar_gz_archive_uses_release_layout_and_excludes_scripts(self) -> None:
        """Tar packages contain runtime resources under neser/ and omit dev scripts."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            repo_root = Path(temp_dir_str)
            output_dir = repo_root / "dist"
            binary_path = repo_root / "target/release/neser"
            write_bytes(binary_path, b"#!/bin/sh\n")
            binary_path.chmod(0o755)
            write_bytes(repo_root / "assets/fonts/NunitoSans-Regular.ttf")
            write_text(repo_root / "gamecontrollerdb.txt", "controller mappings\n")
            write_text(repo_root / "neser.conf.example", "# config\n")
            write_text(repo_root / "README.md", "# neser\n")
            write_text(repo_root / "LICENSE", "license\n")
            write_text(repo_root / "shaders/stock.slangp", "shader0 = stock.slang\n")
            write_text(repo_root / "shaders/stock.slang", "void main() {}\n")
            write_text(
                repo_root / "src/platform/shaders.rs",
                'pub const SHADER_PRESETS: &[(&str, &str)] = &[("none", '
                '"shaders/stock.slangp")];',
            )
            write_text(repo_root / "scripts/dev-only.py", "print('dev only')\n")

            archive_path = create_release_archive(
                ReleasePackageConfig(
                    repo_root=repo_root,
                    binary_path=binary_path,
                    output_dir=output_dir,
                    target="x86_64-unknown-linux-gnu",
                    archive_format="tar.gz",
                )
            )

            self.assertEqual(archive_path.name, "neser-x86_64-unknown-linux-gnu.tar.gz")
            with tarfile.open(archive_path, "r:gz") as archive:
                names = set(archive.getnames())

                self.assertIn("neser/neser", names)
                self.assertIn("neser/assets/fonts/NunitoSans-Regular.ttf", names)
                self.assertIn("neser/gamecontrollerdb.txt", names)
                self.assertIn("neser/neser.conf.example", names)
                self.assertIn("neser/README.md", names)
                self.assertIn("neser/LICENSE", names)
                self.assertIn("neser/shaders/stock.slangp", names)
                self.assertIn("neser/shaders/stock.slang", names)
                self.assertFalse(any(name.startswith("neser/scripts/") for name in names))

                binary_info = archive.getmember("neser/neser")
                self.assertTrue(binary_info.mode & stat.S_IXUSR)

    def test_create_zip_archive_uses_windows_release_layout(self) -> None:
        """Zip packages contain Windows binary and runtime resources under neser/."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            repo_root = Path(temp_dir_str)
            output_dir = repo_root / "dist"
            binary_path = repo_root / "target/release/neser.exe"
            write_bytes(binary_path, b"MZ")
            write_bytes(repo_root / "assets/fonts/NunitoSans-Regular.ttf")
            write_text(repo_root / "gamecontrollerdb.txt", "controller mappings\n")
            write_text(repo_root / "neser.conf.example", "# config\n")
            write_text(repo_root / "README.md", "# neser\n")
            write_text(repo_root / "LICENSE", "license\n")
            write_text(repo_root / "shaders/stock.slangp", "shader0 = stock.slang\n")
            write_text(repo_root / "shaders/stock.slang", "void main() {}\n")
            write_text(
                repo_root / "src/platform/shaders.rs",
                'pub const SHADER_PRESETS: &[(&str, &str)] = &[("none", '
                '"shaders/stock.slangp")];',
            )
            write_text(repo_root / "scripts/dev-only.py", "print('dev only')\n")

            archive_path = create_release_archive(
                ReleasePackageConfig(
                    repo_root=repo_root,
                    binary_path=binary_path,
                    output_dir=output_dir,
                    target="x86_64-pc-windows-msvc",
                    archive_format="zip",
                    binary_name="neser.exe",
                )
            )

            self.assertEqual(archive_path.name, "neser-x86_64-pc-windows-msvc.zip")
            with zipfile.ZipFile(archive_path) as archive:
                names = set(archive.namelist())

            self.assertIn("neser/neser.exe", names)
            self.assertIn("neser/gamecontrollerdb.txt", names)
            self.assertIn("neser/shaders/stock.slangp", names)
            self.assertFalse(any(name.startswith("neser/scripts/") for name in names))


if __name__ == "__main__":
    unittest.main()