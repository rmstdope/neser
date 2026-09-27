"""Unit tests for the release preparation script."""

import datetime
import tempfile
import unittest
from pathlib import Path

from scripts.prepare_release import (
    ReleaseKind,
    apply_release,
    bump_version,
    format_release_date,
    main,
    render_scroller_sentence,
    update_cargo_lock,
    update_cargo_toml,
    update_scroller_text,
)


class BumpVersionTests(unittest.TestCase):
    def test_maintenance_steps_the_third_digit(self) -> None:
        self.assertEqual(bump_version("1.2.0", ReleaseKind.MAINTENANCE), "1.2.1")

    def test_minor_steps_the_second_digit_and_resets_the_third(self) -> None:
        self.assertEqual(bump_version("1.2.7", ReleaseKind.MINOR), "1.3.0")

    def test_major_steps_the_first_digit_and_clears_the_others(self) -> None:
        self.assertEqual(bump_version("1.2.7", ReleaseKind.MAJOR), "2.0.0")

    def test_rejects_a_version_that_is_not_three_numbers(self) -> None:
        with self.assertRaises(ValueError):
            bump_version("1.2", ReleaseKind.MINOR)
        with self.assertRaises(ValueError):
            bump_version("v1.2.0", ReleaseKind.MINOR)


class ScrollerSentenceTests(unittest.TestCase):
    def test_date_reads_as_month_day_year_without_zero_padding(self) -> None:
        self.assertEqual(format_release_date(datetime.date(2026, 9, 7)), "September 7, 2026")

    def test_sentence_keeps_the_shape_of_the_existing_scroll_text(self) -> None:
        sentence = render_scroller_sentence(
            datetime.date(2026, 9, 27), "1.3.0", "SNES games with SA-1 and SuperFX. Super Scope with the mouse."
        )
        self.assertEqual(
            sentence,
            "September 27, 2026: Version 1.3.0 - SNES games with SA-1 and SuperFX. Super Scope with the mouse.",
        )

    def test_highlights_are_trimmed_and_double_quotes_rejected(self) -> None:
        self.assertTrue(render_scroller_sentence(datetime.date(2026, 1, 1), "1.0.0", "  Hello.  ").endswith("- Hello."))
        with self.assertRaises(ValueError):
            render_scroller_sentence(datetime.date(2026, 1, 1), "1.0.0", 'say "hi"')


CARGO_TOML = "\n".join(
    [
        "[package]",
        'name = "neser"',
        'version = "1.2.0"',
        'edition = "2024"',
        "",
        "[dependencies]",
        'foo = { version = "1.2.0" }',
        "",
    ]
)
CARGO_LOCK = "\n".join(
    [
        "[[package]]",
        'name = "foo"',
        'version = "1.2.0"',
        "",
        "[[package]]",
        'name = "neser"',
        'version = "1.2.0"',
        "dependencies = [",
        ' "foo",',
        "]",
        "",
    ]
)
APP_TS = (
    'const SCROLLER_TEXT = "May 26, 2026: Version 1.1.0 - GB (DMG+CGB) emulator in ok state.";\n'
    "const SCROLLER_SPEED = 1.6;\n"
)


class FileEditTests(unittest.TestCase):
    def test_cargo_toml_changes_only_the_package_version(self) -> None:
        updated = update_cargo_toml(CARGO_TOML, "1.3.0")
        self.assertIn('version = "1.3.0"\nedition', updated)
        self.assertIn('foo = { version = "1.2.0" }', updated)

    def test_cargo_lock_changes_only_the_neser_package(self) -> None:
        updated = update_cargo_lock(CARGO_LOCK, "1.3.0")
        self.assertIn('name = "neser"\nversion = "1.3.0"', updated)
        self.assertIn('name = "foo"\nversion = "1.2.0"', updated)

    def test_scroller_text_is_replaced_whole(self) -> None:
        updated = update_scroller_text(APP_TS, "September 27, 2026: Version 1.3.0 - New things.")
        self.assertEqual(
            updated,
            'const SCROLLER_TEXT = "September 27, 2026: Version 1.3.0 - New things.";\nconst SCROLLER_SPEED = 1.6;\n',
        )

    def test_missing_anchors_fail_loudly(self) -> None:
        with self.assertRaises(ValueError):
            update_cargo_toml('[package]\nname = "other"\n', "1.3.0")
        with self.assertRaises(ValueError):
            update_cargo_lock('[[package]]\nname = "foo"\nversion = "1.0.0"\n', "1.3.0")
        with self.assertRaises(ValueError):
            update_scroller_text("const OTHER = 1;\n", "x")


class ApplyReleaseTests(unittest.TestCase):
    def make_repo(self) -> Path:
        root = Path(tempfile.mkdtemp())
        (root / "Cargo.toml").write_text(CARGO_TOML, encoding="utf-8")
        (root / "Cargo.lock").write_text(CARGO_LOCK, encoding="utf-8")
        (root / "web" / "src").mkdir(parents=True)
        (root / "web" / "src" / "app.ts").write_text(APP_TS, encoding="utf-8")
        (root / "notes.md").write_text("# NESER v1.3.0\n\nNotes.\n", encoding="utf-8")
        return root

    def test_apply_writes_every_file_and_the_notes_under_docs_releases(self) -> None:
        root = self.make_repo()
        version = apply_release(
            root,
            ReleaseKind.MINOR,
            highlights="New things.",
            notes_path=root / "notes.md",
            today=datetime.date(2026, 9, 27),
        )
        self.assertEqual(version, "1.3.0")
        self.assertIn('version = "1.3.0"\nedition', (root / "Cargo.toml").read_text(encoding="utf-8"))
        self.assertIn('name = "neser"\nversion = "1.3.0"', (root / "Cargo.lock").read_text(encoding="utf-8"))
        self.assertIn(
            'SCROLLER_TEXT = "September 27, 2026: Version 1.3.0 - New things."',
            (root / "web" / "src" / "app.ts").read_text(encoding="utf-8"),
        )
        self.assertEqual(
            (root / "docs" / "releases" / "v1.3.0.md").read_text(encoding="utf-8"), "# NESER v1.3.0\n\nNotes.\n"
        )

    def test_apply_refuses_to_overwrite_existing_notes(self) -> None:
        root = self.make_repo()
        (root / "docs" / "releases").mkdir(parents=True)
        (root / "docs" / "releases" / "v1.3.0.md").write_text("old\n", encoding="utf-8")
        with self.assertRaises(FileExistsError):
            apply_release(
                root, ReleaseKind.MINOR, highlights="x", notes_path=root / "notes.md", today=datetime.date(2026, 9, 27)
            )
        self.assertIn('version = "1.2.0"', (root / "Cargo.toml").read_text(encoding="utf-8"))


class MainTests(unittest.TestCase):
    def test_print_version_only_reports_the_next_version(self) -> None:
        root = ApplyReleaseTests().make_repo()
        import contextlib
        import io

        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = main(["--repo-root", str(root), "--kind", "major", "--print-version"])
        self.assertEqual(code, 0)
        self.assertEqual(out.getvalue().strip(), "2.0.0")
        self.assertIn('version = "1.2.0"', (root / "Cargo.toml").read_text(encoding="utf-8"))

    def test_apply_requires_highlights_and_notes(self) -> None:
        root = ApplyReleaseTests().make_repo()
        with self.assertRaises(SystemExit):
            main(["--repo-root", str(root), "--kind", "minor"])


if __name__ == "__main__":
    unittest.main()
