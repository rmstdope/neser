"""UI and interaction tests for MapperToolApp."""

import asyncio
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest

from textual.widgets import DataTable
from textual.widgets import Input
from textual.widgets import TextArea

from .constants import REPO_ROOT
from .mapper_tool_app import MapperToolApp
from .test_helpers import make_ines_rom


class MapperToolAppLayoutTests(unittest.TestCase):
    """Validate widget composition and user interactions."""

    def test_css_defines_top_and_three_pane_layout(self) -> None:
        """CSS establishes a full-width top bar and three equal bottom panes."""

        css = MapperToolApp.CSS
        self.assertIn("#autorun-progress {", css)
        self.assertIn("height: 3;", css)
        self.assertIn("#main-panes {", css)
        self.assertIn("layout: horizontal;", css)
        self.assertIn(".pane {", css)
        self.assertIn("width: 1fr;", css)

    def test_app_mounts_required_widgets(self) -> None:
        """All required widgets exist after the app is mounted."""

        async def run_assertions() -> None:
            app = MapperToolApp()
            async with app.run_test() as pilot:
                await pilot.pause()
                self.assertIsNotNone(app.query_one("#autorun-progress"))
                self.assertIsNotNone(app.query_one("#rom-database"))
                self.assertIsNotNone(app.query_one("#logs"))
                self.assertIsNotNone(app.query_one("#config-editor"))

        asyncio.run(run_assertions())

    def test_rom_table_columns_show_mapper_submapper_before_rom(self) -> None:
        """Left table shows mapper and submapper columns before ROM column."""

        async def run_assertions() -> None:
            app = MapperToolApp()
            async with app.run_test() as pilot:
                await pilot.pause()
                rom_table = app.query_one("#rom-database")
                labels = [column.label.plain for column in rom_table.ordered_columns]
                self.assertEqual(labels, ["Map", "SMap", "ROM", "CRC", "Source"])

        asyncio.run(run_assertions())

    def test_rom_table_has_inline_filter_inputs(self) -> None:
        """ROM pane includes mapper and name filter inputs above the table."""

        async def run_assertions() -> None:
            app = MapperToolApp()
            async with app.run_test() as pilot:
                await pilot.pause()
                self.assertIsNotNone(app.query_one("#mapper-filter-input", Input))
                self.assertIsNotNone(app.query_one("#name-filter-input", Input))

        asyncio.run(run_assertions())

    def test_logs_widget_is_selectable_read_only_textarea(self) -> None:
        """Logs pane uses a read-only TextArea so text can be marked and copied."""

        async def run_assertions() -> None:
            app = MapperToolApp()
            async with app.run_test() as pilot:
                await pilot.pause()
                logs = app.query_one("#logs", TextArea)
                self.assertTrue(logs.read_only)

        asyncio.run(run_assertions())

    def test_app_registers_log_copy_bindings(self) -> None:
        """App exposes key bindings for copying selected log text."""

        keys = {binding[0] for binding in MapperToolApp.BINDINGS}
        self.assertIn("meta+c", keys)
        self.assertIn("ctrl+shift+c", keys)

    def test_default_paths_are_resolved_from_repo_root(self) -> None:
        """Relative defaults resolve against repository root, not process cwd."""

        app = MapperToolApp()
        self.assertEqual(app.rom_db_csv_path, REPO_ROOT / MapperToolApp.DEFAULT_ROM_DB_PATH)
        self.assertEqual(app.rom_root, REPO_ROOT / MapperToolApp.DEFAULT_ROM_ROOT)
        self.assertEqual(
            app.rom_files_csv_path,
            REPO_ROOT / MapperToolApp.DEFAULT_ROM_FILES_DB_PATH,
        )

    def test_copy_log_selection_action_copies_selected_text(self) -> None:
        """Copy action sends current log selection to clipboard."""

        async def run_assertions() -> None:
            app = MapperToolApp()
            copied: list[str] = []
            app.copy_to_clipboard = copied.append  # type: ignore[method-assign]

            async with app.run_test() as pilot:
                await pilot.pause()
                logs = app.query_one("#logs", TextArea)
                logs.select_all()
                app.action_copy_log_selection()
                self.assertTrue(copied)
                self.assertIn("Mappertool logs will appear here.", copied[0])

        asyncio.run(run_assertions())

    def test_app_loads_rom_db_on_startup(self) -> None:
        """App startup reads ROM DB and enables CRC lookups."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            csv_path = Path(temp_dir_str) / "rom_db.csv"
            csv_path.write_text(
                "1,Demo Game,,44D21F83,0,0,Licensed Japan,1,0,H,0,0,0,0,0,0,0,0,0,,,1\n",
                encoding="utf-8",
            )

            async def run_assertions() -> None:
                app = MapperToolApp(rom_db_csv_path=csv_path)
                async with app.run_test() as pilot:
                    await pilot.pause()
                    entry = app.lookup_rom_by_crc(0x44D21F83)
                    self.assertIsNotNone(entry)
                    assert entry is not None
                    self.assertEqual(entry.name, "Demo Game")

            asyncio.run(run_assertions())

    def test_header_sort_toggles_mapper_column_direction(self) -> None:
        """Pressing mapper header toggles ascending then descending sort."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)

            (rom_root / "b.nes").write_bytes(make_ines_rom(mapper=10, submapper=0))
            (rom_root / "a.nes").write_bytes(make_ines_rom(mapper=1, submapper=0))
            (rom_root / "c.nes").write_bytes(make_ines_rom(mapper=2, submapper=0))

            rom_db_path = temp_root / "rom_db.csv"
            rom_db_path.write_text("# empty\n", encoding="utf-8")
            rom_files_db_path = temp_root / "rom_files.csv"

            async def run_assertions() -> None:
                app = MapperToolApp(
                    rom_db_csv_path=rom_db_path,
                    rom_root=rom_root,
                    rom_files_csv_path=rom_files_db_path,
                )
                async with app.run_test() as pilot:
                    await pilot.pause()
                    table = app.query_one("#rom-database", DataTable)
                    mapper_column = table.ordered_columns[0]

                    app.on_data_table_header_selected(
                        DataTable.HeaderSelected(table, mapper_column.key, 0, mapper_column.label)
                    )
                    self.assertEqual(str(table.get_row_at(0)[0]), "1")

                    app.on_data_table_header_selected(
                        DataTable.HeaderSelected(table, mapper_column.key, 0, mapper_column.label)
                    )
                    self.assertEqual(str(table.get_row_at(0)[0]), "10")

            asyncio.run(run_assertions())

    def test_header_sort_toggles_name_column_direction(self) -> None:
        """Pressing ROM name header toggles ascending then descending name sort."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)

            (rom_root / "b_name.nes").write_bytes(make_ines_rom(mapper=2, submapper=1))
            (rom_root / "a_name.nes").write_bytes(make_ines_rom(mapper=1, submapper=2))
            (rom_root / "c_name.nes").write_bytes(make_ines_rom(mapper=1, submapper=1))

            rom_db_path = temp_root / "rom_db.csv"
            rom_db_path.write_text("# empty\n", encoding="utf-8")
            rom_files_db_path = temp_root / "rom_files.csv"

            async def run_assertions() -> None:
                app = MapperToolApp(
                    rom_db_csv_path=rom_db_path,
                    rom_root=rom_root,
                    rom_files_csv_path=rom_files_db_path,
                )
                async with app.run_test() as pilot:
                    await pilot.pause()
                    table = app.query_one("#rom-database", DataTable)
                    rom_column = table.ordered_columns[2]

                    app.on_data_table_header_selected(
                        DataTable.HeaderSelected(table, rom_column.key, 2, rom_column.label)
                    )
                    self.assertEqual(str(table.get_row_at(0)[2]), "a_name.nes")

                    app.on_data_table_header_selected(
                        DataTable.HeaderSelected(table, rom_column.key, 2, rom_column.label)
                    )
                    self.assertEqual(str(table.get_row_at(0)[2]), "c_name.nes")

            asyncio.run(run_assertions())

    def test_live_name_filter_updates_table_while_typing(self) -> None:
        """Name filter input updates visible rows immediately while typing."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)

            (rom_root / "super_mario.nes").write_bytes(make_ines_rom(mapper=2, submapper=0))
            (rom_root / "zelda.nes").write_bytes(make_ines_rom(mapper=1, submapper=0))

            rom_db_path = temp_root / "rom_db.csv"
            rom_db_path.write_text("# empty\n", encoding="utf-8")
            rom_files_db_path = temp_root / "rom_files.csv"

            async def run_assertions() -> None:
                app = MapperToolApp(
                    rom_db_csv_path=rom_db_path,
                    rom_root=rom_root,
                    rom_files_csv_path=rom_files_db_path,
                )
                async with app.run_test() as pilot:
                    await pilot.pause()
                    table = app.query_one("#rom-database", DataTable)
                    name_filter = app.query_one("#name-filter-input", Input)
                    self.assertEqual(table.row_count, 2)

                    name_filter.value = "mario"
                    app.on_input_changed(SimpleNamespace(input=name_filter, value="mario"))
                    self.assertEqual(table.row_count, 1)
                    self.assertEqual(str(table.get_row_at(0)[2]), "super_mario.nes")

            asyncio.run(run_assertions())

    def test_live_mapper_filter_uses_exact_comma_separated_values(self) -> None:
        """Mapper filter input matches exact mapper values from comma-separated list."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)

            (rom_root / "map24.nes").write_bytes(make_ines_rom(mapper=24, submapper=2))
            (rom_root / "map25.nes").write_bytes(make_ines_rom(mapper=25, submapper=0))
            (rom_root / "map241.nes").write_bytes(make_ines_rom(mapper=241, submapper=0))

            rom_db_path = temp_root / "rom_db.csv"
            rom_db_path.write_text("# empty\n", encoding="utf-8")
            rom_files_db_path = temp_root / "rom_files.csv"

            async def run_assertions() -> None:
                app = MapperToolApp(
                    rom_db_csv_path=rom_db_path,
                    rom_root=rom_root,
                    rom_files_csv_path=rom_files_db_path,
                )
                async with app.run_test() as pilot:
                    await pilot.pause()
                    table = app.query_one("#rom-database", DataTable)
                    mapper_filter = app.query_one("#mapper-filter-input", Input)
                    self.assertEqual(table.row_count, 3)

                    mapper_filter.value = "24,25"
                    app.on_input_changed(SimpleNamespace(input=mapper_filter, value="24,25"))
                    self.assertEqual(table.row_count, 2)

                    mapper_filter.value = "241"
                    app.on_input_changed(SimpleNamespace(input=mapper_filter, value="241"))
                    self.assertEqual(table.row_count, 1)
                    self.assertEqual(str(table.get_row_at(0)[0]), "241")

            asyncio.run(run_assertions())

    def test_parse_mapper_filter_values_parses_comma_separated_exact_numbers(self) -> None:
        """Mapper parser keeps integer values from comma-separated input."""

        self.assertEqual(MapperToolApp.parse_mapper_filter_values("24,30,241"), {24, 30, 241})
        self.assertEqual(MapperToolApp.parse_mapper_filter_values("24, bad, 30"), {24, 30})

    def test_rom_column_displays_filename_only(self) -> None:
        """ROM column shows filename while preserving full path internally."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            (rom_root / "nested").mkdir(parents=True)

            (rom_root / "nested" / "sample.nes").write_bytes(make_ines_rom(mapper=2, submapper=0))

            rom_db_path = temp_root / "rom_db.csv"
            rom_db_path.write_text("# empty\n", encoding="utf-8")
            rom_files_db_path = temp_root / "rom_files.csv"

            async def run_assertions() -> None:
                app = MapperToolApp(
                    rom_db_csv_path=rom_db_path,
                    rom_root=rom_root,
                    rom_files_csv_path=rom_files_db_path,
                )
                async with app.run_test() as pilot:
                    await pilot.pause()
                    table = app.query_one("#rom-database", DataTable)
                    self.assertEqual(str(table.get_row_at(0)[2]), "sample.nes")

            asyncio.run(run_assertions())

    def test_row_highlight_sets_tooltip_to_full_rom_path(self) -> None:
        """Highlighting a ROM row sets tooltip to the full stored ROM path."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            (rom_root / "nested").mkdir(parents=True)

            (rom_root / "nested" / "sample.nes").write_bytes(make_ines_rom(mapper=2, submapper=0))

            rom_db_path = temp_root / "rom_db.csv"
            rom_db_path.write_text("# empty\n", encoding="utf-8")
            rom_files_db_path = temp_root / "rom_files.csv"

            async def run_assertions() -> None:
                app = MapperToolApp(
                    rom_db_csv_path=rom_db_path,
                    rom_root=rom_root,
                    rom_files_csv_path=rom_files_db_path,
                )
                async with app.run_test() as pilot:
                    await pilot.pause()
                    table = app.query_one("#rom-database", DataTable)

                    app.on_data_table_row_highlighted(
                        SimpleNamespace(data_table=table, cursor_row=0)
                    )

                    self.assertEqual(table.tooltip, "nested/sample.nes")

            asyncio.run(run_assertions())
