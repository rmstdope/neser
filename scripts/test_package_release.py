"""Unit tests for release package assembly."""

import stat
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path

from scripts.package_release import (
    ReleasePackageConfig,
    _read_shader_preset_paths,
    collect_shader_dependencies,
    create_release_archive,
    main,
)


def write_text(path: Path, text: str) -> None:
    """Write UTF-8 text, creating parent directories first."""

    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def write_bytes(path: Path, data: bytes = b"test") -> None:
    """Write bytes, creating parent directories first."""

    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)


def path_exists_with_exact_case(repo_root: Path, relative_path: Path) -> bool:
    """Return whether each path segment matches the directory entry casing exactly."""

    current = repo_root
    for part in relative_path.parts:
        if not current.is_dir():
            return False
        entries = {entry.name for entry in current.iterdir()}
        if part not in entries:
            return False
        current = current / part
    return current.exists()


class PackageReleaseTests(unittest.TestCase):
    """Behavior tests for release archive creation."""

    def test_repository_shader_dependencies_are_collectable(self) -> None:
        """Release packaging can collect every shader preset configured by the repository."""

        repo_root = Path(__file__).resolve().parents[1]

        dependencies = collect_shader_dependencies(repo_root)

        self.assertIn(Path("shaders/stock.slangp"), dependencies)

    def test_repository_shader_preset_paths_match_exact_vendor_casing(self) -> None:
        """Configured shader preset paths match vendored filenames on case-sensitive runners."""

        repo_root = Path(__file__).resolve().parents[1]
        preset_paths = _read_shader_preset_paths(repo_root / "src/platform/shaders.rs")

        mismatches = [path for path in preset_paths if not path_exists_with_exact_case(repo_root, path)]

        self.assertEqual(mismatches, [])

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
            # `textures` lists texture *names*; each name then has its own line
            # giving the path. Earlier revisions of this fixture wrote
            # `textures = "lut.png"`, which no real preset does.
            write_text(
                repo_root / "vendor/slang-shaders/crt/preset.slangp",
                '#reference "../common/base.slangp"\nshader0 = pass.slang\ntextures = "LUT"\nLUT = lut.png\n',
            )
            write_text(
                repo_root / "vendor/slang-shaders/crt/pass.slang",
                '#include "shared.inc"\n',
            )
            write_text(repo_root / "vendor/slang-shaders/crt/shared.inc", "#define TEST 1\n")
            write_bytes(repo_root / "vendor/slang-shaders/crt/lut.png")
            write_text(
                repo_root / "vendor/slang-shaders/common/base.slangp",
                "shader0 = base.slang\n",
            )
            write_text(repo_root / "vendor/slang-shaders/common/base.slang", "void main() {}\n")

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

    def _write_preset_repo(self, repo_root: Path, preset_body: str) -> None:
        """Write a minimal repo whose single configured preset has `preset_body`."""

        write_text(
            repo_root / "src/platform/shaders.rs",
            "pub const SHADER_PRESETS: &[(&str, &str)] = &[\n"
            '    ("crt", "vendor/slang-shaders/crt/preset.slangp"),\n'
            "];\n",
        )
        write_text(repo_root / "vendor/slang-shaders/crt/preset.slangp", preset_body)
        write_text(repo_root / "vendor/slang-shaders/crt/pass.slang", "void main() {}\n")

    def test_declared_texture_that_is_missing_fails_collection(self) -> None:
        """A texture the preset declares must resolve, like any other reference.

        Missing `.slangp`/`.slang`/`.inc` targets already raise. Textures used to
        be matched by a loose regex and then filtered on `exists()`, which cannot
        tell a renamed texture from a regex false positive -- so a preset could be
        packaged without its lookup table, silently (#3123).
        """

        with tempfile.TemporaryDirectory() as temp_dir_str:
            repo_root = Path(temp_dir_str)
            self._write_preset_repo(
                repo_root,
                "shader0 = pass.slang\ntextures = LUT\nLUT = lut/renamed-away.png\n",
            )

            with self.assertRaises(FileNotFoundError) as caught:
                collect_shader_dependencies(repo_root)

            self.assertIn("renamed-away.png", str(caught.exception))

    def test_quoted_textures_declaration_resolves_paths_and_ignores_option_keys(self) -> None:
        """Quoted `textures = "A;B"` resolves both paths, not the adjacent option keys.

        Real presets interleave `<name>_linear`, `<name>_mipmap` and
        `<name>_wrap_mode` with the path lines, so a declared name has to match a
        key exactly rather than as a prefix.
        """

        with tempfile.TemporaryDirectory() as temp_dir_str:
            repo_root = Path(temp_dir_str)
            self._write_preset_repo(
                repo_root,
                "shader0 = pass.slang\n"
                'textures = "SamplerLUT1;SamplerLUT2"\n'
                "SamplerLUT1 = lut/first.png\n"
                'SamplerLUT1_mipmap = "false"\n'
                'SamplerLUT1_linear = "true"\n'
                'SamplerLUT1_wrap_mode = "clamp_to_border"\n'
                # Deliberately before SamplerLUT2's own line: a parser that
                # matched a declared name as a prefix would resolve "true" as
                # the path here instead of the .png below.
                'SamplerLUT2_linear = "true"\n'
                'SamplerLUT2 = "lut/second one.png"\n',
            )
            write_bytes(repo_root / "vendor/slang-shaders/crt/lut/first.png")
            write_bytes(repo_root / "vendor/slang-shaders/crt/lut/second one.png")

            dependencies = collect_shader_dependencies(repo_root)

            self.assertIn(Path("vendor/slang-shaders/crt/lut/first.png"), dependencies)
            self.assertIn(Path("vendor/slang-shaders/crt/lut/second one.png"), dependencies)

    def test_undeclared_image_line_does_not_fail_collection(self) -> None:
        """A line that merely looks like an image path stays best-effort.

        The loose regexes remain as a fallback for images referenced some other
        way, so an absent one is skipped rather than raising -- only *declared*
        textures are strict.
        """

        with tempfile.TemporaryDirectory() as temp_dir_str:
            repo_root = Path(temp_dir_str)
            self._write_preset_repo(
                repo_root,
                "shader0 = pass.slang\nsome_unrelated_key = not-a-texture.png\n",
            )

            dependencies = collect_shader_dependencies(repo_root)

            self.assertNotIn(Path("vendor/slang-shaders/crt/not-a-texture.png"), dependencies)

    def test_declared_texture_without_a_path_line_fails_collection(self) -> None:
        """A declared name that is never assigned a path is a broken declaration.

        Resolving only the `<name> = path` lines that happen to be present would
        leave this silent, which is the failure mode #3123 exists to remove: the
        preset promises a texture and packaging ships without it.
        """

        with tempfile.TemporaryDirectory() as temp_dir_str:
            repo_root = Path(temp_dir_str)
            self._write_preset_repo(
                repo_root,
                'shader0 = pass.slang\ntextures = LUT\nLUT_linear = "true"\n',
            )

            with self.assertRaises(ValueError) as caught:
                collect_shader_dependencies(repo_root)

            self.assertIn("LUT", str(caught.exception))

    def test_create_tar_gz_archive_uses_release_layout_and_excludes_scripts(
        self,
    ) -> None:
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
            write_text(repo_root / "README-NES.md", "# nes\n")
            write_text(repo_root / "README-GB.md", "# gb\n")
            write_text(repo_root / "README-GBA.md", "# gba\n")
            write_text(repo_root / "LICENSE", "license\n")
            write_text(repo_root / "shaders/stock.slangp", "shader0 = stock.slang\n")
            write_text(repo_root / "shaders/stock.slang", "void main() {}\n")
            write_text(
                repo_root / "src/platform/shaders.rs",
                'pub const SHADER_PRESETS: &[(&str, &str)] = &[("none", "shaders/stock.slangp")];',
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
                self.assertIn("neser/README-NES.md", names)
                self.assertIn("neser/README-GB.md", names)
                self.assertIn("neser/README-GBA.md", names)
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
            write_text(repo_root / "README-NES.md", "# nes\n")
            write_text(repo_root / "README-GB.md", "# gb\n")
            write_text(repo_root / "README-GBA.md", "# gba\n")
            write_text(repo_root / "LICENSE", "license\n")
            write_text(repo_root / "shaders/stock.slangp", "shader0 = stock.slang\n")
            write_text(repo_root / "shaders/stock.slang", "void main() {}\n")
            write_text(
                repo_root / "src/platform/shaders.rs",
                'pub const SHADER_PRESETS: &[(&str, &str)] = &[("none", "shaders/stock.slangp")];',
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
            self.assertIn("neser/README-NES.md", names)
            self.assertIn("neser/README-GB.md", names)
            self.assertIn("neser/README-GBA.md", names)
            self.assertIn("neser/shaders/stock.slangp", names)
            self.assertFalse(any(name.startswith("neser/scripts/") for name in names))

    def test_main_creates_archive_from_cli_arguments(self) -> None:
        """CLI arguments create the requested release archive."""

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
            write_text(repo_root / "README-NES.md", "# nes\n")
            write_text(repo_root / "README-GB.md", "# gb\n")
            write_text(repo_root / "README-GBA.md", "# gba\n")
            write_text(repo_root / "LICENSE", "license\n")
            write_text(repo_root / "shaders/stock.slangp", "shader0 = stock.slang\n")
            write_text(repo_root / "shaders/stock.slang", "void main() {}\n")
            write_text(
                repo_root / "src/platform/shaders.rs",
                'pub const SHADER_PRESETS: &[(&str, &str)] = &[("none", "shaders/stock.slangp")];',
            )

            exit_code = main(
                [
                    "--repo-root",
                    str(repo_root),
                    "--binary",
                    str(binary_path),
                    "--output-dir",
                    str(output_dir),
                    "--target",
                    "x86_64-unknown-linux-gnu",
                    "--format",
                    "tar.gz",
                ]
            )

            self.assertEqual(exit_code, 0)
            self.assertTrue((output_dir / "neser-x86_64-unknown-linux-gnu.tar.gz").exists())


if __name__ == "__main__":
    unittest.main()
