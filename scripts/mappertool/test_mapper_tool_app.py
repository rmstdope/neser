"""UI and interaction tests for MapperToolApp."""

import asyncio
import inspect
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock

from rich.text import Text
from textual.widgets import Button, Checkbox, DataTable, Input, TextArea

from .constants import REPO_ROOT
from .mapper_tool_app import MapperToolApp
from .test_helpers import make_ines_rom


def _install_run_worker_mock(app: MapperToolApp) -> Mock:
    """Replace run_worker with mock that closes coroutine args to avoid warnings."""

    worker_mock = Mock()

    def close_coro_and_return(*args, **kwargs):
        if args and inspect.iscoroutine(args[0]):
            args[0].close()
        return None

    worker_mock.side_effect = close_coro_and_return
    app.run_worker = worker_mock  # type: ignore[method-assign]
    return worker_mock


class MapperToolAppLayoutTests(unittest.TestCase):
    """Validate widget composition and user interactions."""

    def test_css_defines_top_bottom_layout_with_60_40_split(self) -> None:
        """CSS establishes top ROM/config split and bottom horizontal logs row."""

        css = MapperToolApp.CSS
        self.assertIn("#top-panes {", css)
        self.assertIn("#bottom-panes {", css)
        self.assertIn("layout: horizontal;", css)
        self.assertIn("#rom-pane {", css)
        self.assertIn("width: 3fr;", css)
        self.assertIn("#rom-database {", css)
        self.assertIn("height: 1fr;", css)
        self.assertIn("#config-editor {", css)
        self.assertIn("width: 2fr;", css)
        self.assertIn("border: solid $accent;", css)

    def test_app_mounts_required_widgets(self) -> None:
        """All required widgets exist after the app is mounted."""

        async def run_assertions() -> None:
            app = MapperToolApp()
            async with app.run_test() as pilot:
                await pilot.pause()
                self.assertIsNotNone(app.query_one("#top-panes"))
                self.assertIsNotNone(app.query_one("#bottom-panes"))
                self.assertIsNotNone(app.query_one("#rom-database"))
                self.assertIsNotNone(app.query_one("#logs"))
                self.assertIsNotNone(app.query_one("#config-editor"))

        asyncio.run(run_assertions())

    def test_pane_frames_show_inline_titles(self) -> None:
        """All three framed panes expose inline frame titles on their borders."""

        async def run_assertions() -> None:
            app = MapperToolApp()
            async with app.run_test() as pilot:
                await pilot.pause()
                self.assertEqual(app.query_one("#rom-pane").border_title, app.ROM_PANE_TITLE)
                self.assertEqual(app.query_one("#config-editor").border_title, app.CONFIG_PANE_TITLE)
                self.assertEqual(app.query_one("#logs-pane").border_title, app.LOGS_PANE_TITLE)
                self.assertEqual(app.CONFIG_PANE_TITLE, "Actions")

        asyncio.run(run_assertions())

    def test_rom_table_columns_show_mapper_submapper_before_rom(self) -> None:
        """Left table shows mapper and submapper columns before ROM column."""

        async def run_assertions() -> None:
            app = MapperToolApp()
            async with app.run_test() as pilot:
                await pilot.pause()
                rom_table = app.query_one("#rom-database")
                labels = [column.label.plain for column in rom_table.ordered_columns]
                self.assertEqual(labels, ["Map", "SMap", "ROM", "Autorun", "CRC", "Source"])

        asyncio.run(run_assertions())

    def test_rom_table_uses_row_cursor_selection(self) -> None:
        """ROM table uses row cursor so selecting an entry selects the entire row."""

        async def run_assertions() -> None:
            app = MapperToolApp()
            async with app.run_test() as pilot:
                await pilot.pause()
                rom_table = app.query_one("#rom-database", DataTable)
                self.assertEqual(rom_table.cursor_type, "row")

        asyncio.run(run_assertions())

    def test_rom_table_has_inline_filter_inputs(self) -> None:
        """ROM pane includes mapper and name filter inputs above the table."""

        async def run_assertions() -> None:
            app = MapperToolApp()
            async with app.run_test() as pilot:
                await pilot.pause()
                self.assertIsNotNone(app.query_one("#mapper-filter-input", Input))
                self.assertIsNotNone(app.query_one("#name-filter-input", Input))
                self.assertIsNotNone(app.query_one("#autorun-only-filter", Checkbox))

        asyncio.run(run_assertions())

    def test_rom_filters_place_name_before_mapper(self) -> None:
        """ROM filter row places ROM name input before mapper input."""

        async def run_assertions() -> None:
            app = MapperToolApp()
            async with app.run_test() as pilot:
                await pilot.pause()
                filters_row = app.query_one("#rom-filters")
                child_ids = [child.id for child in filters_row.children]
                self.assertEqual(child_ids, ["name-filter-input", "mapper-filter-input"])

        asyncio.run(run_assertions())

    def test_rom_table_has_initial_focus_on_mount(self) -> None:
        """Without startup scan worker, ROM table receives initial focus on mount."""

        async def run_assertions() -> None:
            app = MapperToolApp()
            _install_run_worker_mock(app)
            async with app.run_test() as pilot:
                await pilot.pause()
                rom_table = app.query_one("#rom-database", DataTable)
                self.assertEqual(app.focused, rom_table)

        asyncio.run(run_assertions())

    def test_config_pane_groups_rom_controls_under_rom_inventory_section(self) -> None:
        """Config pane groups ROM root input and rescan button under ROM Inventory section."""

        async def run_assertions() -> None:
            app = MapperToolApp()
            async with app.run_test() as pilot:
                await pilot.pause()
                section = app.query_one("#rom-inventory-section")
                self.assertEqual(app.query_one("#rom-root-input").parent, section)
                self.assertEqual(app.query_one("#drop-rescan-button").parent, section)
                self.assertEqual(app.query_one("#playback-all-button").parent, section)
                self.assertEqual(app.query_one("#playback-not-run-button").parent, section)
                self.assertEqual(app.query_one("#recalculate-failing-crcs-button").parent, section)
                self.assertEqual(app.query_one("#drop-rescan-button", Button).variant, "warning")
                self.assertEqual(app.query_one("#playback-all-button", Button).variant, "warning")
                self.assertEqual(app.query_one("#playback-all-button", Button).label.plain, "Playback All Recordings")
                self.assertEqual(app.query_one("#playback-not-run-button", Button).variant, "warning")
                self.assertEqual(
                    app.query_one("#recalculate-failing-crcs-button", Button).variant,
                    "warning",
                )

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

        with tempfile.TemporaryDirectory() as temp_dir_str:
            isolated_settings = Path(temp_dir_str) / "mappertool_settings.json"
            app = MapperToolApp(settings_path=isolated_settings)
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
                logs.load_text("sample log line\n")
                logs.select_all()
                app.action_copy_log_selection()
                self.assertTrue(copied)
                self.assertIn("sample log line", copied[0])

        asyncio.run(run_assertions())

    def test_app_loads_rom_db_on_startup(self) -> None:
        """App startup reads ROM DB and enables CRC lookups."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            csv_path = Path(temp_dir_str) / "rom_db.csv"
            csv_path.write_text(
                "1,Demo Game,,44D21F83,0,Licensed Japan,1,0,H,0,0,0,0,0,0,0,0,0,,,1\n",
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

    def test_autorun_only_checkbox_filters_table_rows(self) -> None:
        """Autorun-only checkbox shows only rows with sibling .autorun files."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)

            (rom_root / "with_autorun.nes").write_bytes(make_ines_rom(mapper=24, submapper=0))
            (rom_root / "with_autorun.autorun").write_text("{}", encoding="utf-8")
            (rom_root / "without_autorun.nes").write_bytes(make_ines_rom(mapper=25, submapper=0))

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
                    checkbox = app.query_one("#autorun-only-filter", Checkbox)
                    self.assertEqual(table.row_count, 2)

                    app.on_checkbox_changed(SimpleNamespace(checkbox=checkbox, value=True))
                    self.assertEqual(table.row_count, 1)
                    self.assertEqual(str(table.get_row_at(0)[2]), "with_autorun.nes")

                    app.on_checkbox_changed(SimpleNamespace(checkbox=checkbox, value=False))
                    self.assertEqual(table.row_count, 2)

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

    def test_rom_column_truncates_names_longer_than_thirty_chars(self) -> None:
        """ROM column truncates long names to 30 chars and appends ellipsis."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)

            long_name = "abcdefghijklmnopqrstuvwxyz12345.nes"
            (rom_root / long_name).write_bytes(make_ines_rom(mapper=2, submapper=0))

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
                    displayed_name = str(table.get_row_at(0)[2])
                    self.assertEqual(displayed_name, "abcdefghijklmnopqrstuvwxyz1...")
                    self.assertEqual(len(displayed_name), 30)

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

                    app.on_data_table_row_highlighted(SimpleNamespace(data_table=table, cursor_row=0))

                    self.assertEqual(table.tooltip, "nested/sample.nes")

            asyncio.run(run_assertions())

    def test_row_highlight_requests_cursor_scroll_into_view(self) -> None:
        """Highlight handler asks DataTable to keep highlighted row visible."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)

            (rom_root / "sample.nes").write_bytes(make_ines_rom(mapper=2, submapper=0))

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
                    app.call_after_refresh = Mock()  # type: ignore[method-assign]

                    app.on_data_table_row_highlighted(SimpleNamespace(data_table=table, cursor_row=0))

                    app.call_after_refresh.assert_called_once_with(
                        table._scroll_cursor_into_view,
                        animate=False,
                    )

            asyncio.run(run_assertions())

    def test_highlighted_long_rom_name_scrolls_when_marquee_advances(self) -> None:
        """Highlighted long ROM names scroll horizontally once marquee tick advances."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)

            long_name = "abcdefghijklmnopqrstuvwxyz12345.nes"
            (rom_root / long_name).write_bytes(make_ines_rom(mapper=2, submapper=0))

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
                    initial_display = str(table.get_row_at(0)[2])
                    self.assertEqual(initial_display, "abcdefghijklmnopqrstuvwxyz1...")

                    app.on_data_table_row_highlighted(SimpleNamespace(data_table=table, cursor_row=0))
                    app._rom_name_scroll_active = True
                    app._advance_highlighted_rom_name_scroll()

                    advanced_display = str(table.get_row_at(0)[2])
                    self.assertNotEqual(advanced_display, initial_display)
                    self.assertEqual(len(advanced_display), app.ROM_NAME_MAX_DISPLAY_CHARS)

            asyncio.run(run_assertions())

    def test_row_selection_opens_rom_command_dialog_with_create_when_no_autorun(self) -> None:
        """Selecting ROM row opens command dialog with create action when no autorun exists."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)
            (rom_root / "sample.nes").write_bytes(make_ines_rom(mapper=2, submapper=0))

            rom_db_path = temp_root / "rom_db.csv"
            rom_db_path.write_text("# empty\n", encoding="utf-8")
            rom_files_db_path = temp_root / "rom_files.csv"

            async def run_assertions() -> None:
                app = MapperToolApp(
                    rom_db_csv_path=rom_db_path,
                    rom_root=rom_root,
                    rom_files_csv_path=rom_files_db_path,
                )
                pushed: list[object] = []
                app.push_screen = lambda screen, callback=None: pushed.append(screen)  # type: ignore[method-assign]

                async with app.run_test() as pilot:
                    await pilot.pause()
                    table = app.query_one("#rom-database", DataTable)
                    app.on_data_table_row_selected(SimpleNamespace(data_table=table, cursor_row=0))
                    self.assertTrue(pushed)
                    dialog = pushed[-1]
                    self.assertEqual(type(dialog).__name__, "RomCommandModal")
                    self.assertFalse(dialog.has_autorun)
                    self.assertIsNone(dialog.autorun_summary)

            asyncio.run(run_assertions())

    def test_row_selection_opens_rom_command_dialog_with_playback_when_autorun_exists(self) -> None:
        """Selecting ROM row with autorun opens command dialog in autorun-existing mode."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)
            (rom_root / "sample.nes").write_bytes(make_ines_rom(mapper=2, submapper=0))
            (rom_root / "sample.autorun").write_text(
                '{"version":3,"frames":[{"player1":0,"player2":0,"repeat":12}],"checkpoints":[{"frame_index":11,"screen_crc":1,"state_bytes":[]}]}',
                encoding="utf-8",
            )

            rom_db_path = temp_root / "rom_db.csv"
            rom_db_path.write_text("# empty\n", encoding="utf-8")
            rom_files_db_path = temp_root / "rom_files.csv"

            async def run_assertions() -> None:
                app = MapperToolApp(
                    rom_db_csv_path=rom_db_path,
                    rom_root=rom_root,
                    rom_files_csv_path=rom_files_db_path,
                )
                pushed: list[object] = []
                app.push_screen = lambda screen, callback=None: pushed.append(screen)  # type: ignore[method-assign]

                async with app.run_test() as pilot:
                    await pilot.pause()
                    table = app.query_one("#rom-database", DataTable)
                    app.on_data_table_row_selected(SimpleNamespace(data_table=table, cursor_row=0))
                    self.assertTrue(pushed)
                    dialog = pushed[-1]
                    self.assertEqual(type(dialog).__name__, "RomCommandModal")
                    self.assertTrue(dialog.has_autorun)
                    self.assertIsNotNone(dialog.autorun_summary)
                    self.assertEqual(
                        dialog.autorun_summary,
                        "Autorun: 12 frames, 1 CRCs, Not run",
                    )

            asyncio.run(run_assertions())

    def test_row_selection_dialog_shows_last_run_pass_in_autorun_summary(self) -> None:
        """ROM command dialog summary includes persisted last-run PASS status."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)
            (rom_root / "sample.nes").write_bytes(make_ines_rom(mapper=2, submapper=0))
            (rom_root / "sample.autorun").write_text(
                '{"version":2,"frames":[{"player1":0,"player2":0}],"checkpoints":[]}',
                encoding="utf-8",
            )

            rom_db_path = temp_root / "rom_db.csv"
            rom_db_path.write_text("# empty\n", encoding="utf-8")
            rom_files_db_path = temp_root / "rom_files.csv"

            async def run_assertions() -> None:
                app = MapperToolApp(
                    rom_db_csv_path=rom_db_path,
                    rom_root=rom_root,
                    rom_files_csv_path=rom_files_db_path,
                )
                pushed: list[object] = []
                app.push_screen = lambda screen, callback=None: pushed.append(screen)  # type: ignore[method-assign]

                async with app.run_test() as pilot:
                    await pilot.pause()
                    app._set_record_autorun_status("sample.nes", "passed")
                    table = app.query_one("#rom-database", DataTable)
                    app.on_data_table_row_selected(SimpleNamespace(data_table=table, cursor_row=0))
                    dialog = pushed[-1]
                    self.assertEqual(
                        dialog.autorun_summary,
                        "Autorun: 1 frames, 0 CRCs, PASS",
                    )

            asyncio.run(run_assertions())

    def test_rom_command_modal_button_variants_defined_in_source(self) -> None:
        """ROM dialog source config keeps delete as error and other actions as warning."""

        source = MapperToolApp.RomCommandModal.compose.__code__.co_consts
        joined = " ".join(str(part) for part in source)
        self.assertIn("rom-command-delete", joined)
        self.assertIn("error", joined)
        self.assertIn("rom-command-playback-headless", joined)
        self.assertIn("rom-command-playback-headed", joined)
        self.assertIn("rom-command-run-rom", joined)
        self.assertIn("rom-command-recalculate-crcs", joined)
        self.assertIn("rom-command-extend", joined)
        self.assertIn("rom-command-create", joined)
        self.assertIn("rom-command-cancel", joined)
        self.assertIn("warning", joined)

    def test_run_rom_command_runs_neser_without_autorun_flags(self) -> None:
        """Run ROM dialog action runs neser normally for selected ROM path."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)
            (rom_root / "sample.nes").write_bytes(make_ines_rom(mapper=2, submapper=0))

            rom_db_path = temp_root / "rom_db.csv"
            rom_db_path.write_text("# empty\n", encoding="utf-8")
            rom_files_db_path = temp_root / "rom_files.csv"

            async def run_assertions() -> None:
                app = MapperToolApp(
                    rom_db_csv_path=rom_db_path,
                    rom_root=rom_root,
                    rom_files_csv_path=rom_files_db_path,
                )
                captured_calls: list[tuple[list[str], str, str, str]] = []

                async def fake_run(
                    command: list[str],
                    command_id: str,
                    full_set_progress_status: str = "",
                    current_file_progress_status: str = "",
                ) -> str | None:
                    captured_calls.append(
                        (
                            command,
                            command_id,
                            full_set_progress_status,
                            current_file_progress_status,
                        )
                    )
                    return "passed"

                app._run_autorun_command_with_status_modal = fake_run  # type: ignore[method-assign]

                async with app.run_test() as pilot:
                    await pilot.pause()
                    await app._run_rom_command("sample.nes", "rom-command-run-rom")

                self.assertEqual(len(captured_calls), 1)
                command, command_id, _, _ = captured_calls[0]
                self.assertEqual(command_id, "rom-command-run-rom")
                self.assertNotIn("--playback", command)
                self.assertNotIn("--playback-headless", command)
                self.assertNotIn("--recalculate-autorun", command)
                self.assertTrue(command[-1].endswith("sample.nes"))

            asyncio.run(run_assertions())

    def test_recalculate_crcs_rom_command_runs_recalculate_autorun_flag(self) -> None:
        """Recalculate CRCs dialog action runs neser with --recalculate-autorun for selected ROM."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)
            (rom_root / "sample.nes").write_bytes(make_ines_rom(mapper=2, submapper=0))
            (rom_root / "sample.autorun").write_text("{}", encoding="utf-8")

            rom_db_path = temp_root / "rom_db.csv"
            rom_db_path.write_text("# empty\n", encoding="utf-8")
            rom_files_db_path = temp_root / "rom_files.csv"

            async def run_assertions() -> None:
                app = MapperToolApp(
                    rom_db_csv_path=rom_db_path,
                    rom_root=rom_root,
                    rom_files_csv_path=rom_files_db_path,
                )
                captured_calls: list[tuple[list[str], str, str, str]] = []

                async def fake_run(
                    command: list[str],
                    command_id: str,
                    full_set_progress_status: str = "",
                    current_file_progress_status: str = "",
                ) -> str | None:
                    captured_calls.append(
                        (
                            command,
                            command_id,
                            full_set_progress_status,
                            current_file_progress_status,
                        )
                    )
                    return "passed"

                app._run_autorun_command_with_status_modal = fake_run  # type: ignore[method-assign]

                async with app.run_test() as pilot:
                    await pilot.pause()
                    await app._run_rom_command("sample.nes", "rom-command-recalculate-crcs")

                self.assertEqual(len(captured_calls), 1)
                command, command_id, _, _ = captured_calls[0]
                self.assertEqual(command_id, "rom-command-recalculate-crcs")
                self.assertIn("--recalculate-autorun", command)
                self.assertTrue(command[-1].endswith("sample.nes"))
                self.assertEqual(app.rom_file_records["sample.nes"].autorun_status, "passed")

            asyncio.run(run_assertions())

    def test_rom_command_modal_places_recalculate_directly_above_delete(self) -> None:
        """Autorun command modal orders Recalculate CRCs immediately before Delete recording."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)
            (rom_root / "sample.nes").write_bytes(make_ines_rom(mapper=2, submapper=0))
            (rom_root / "sample.autorun").write_text(
                '{"version":3,"frames":[{"player1":0,"player2":0,"repeat":1}],"checkpoints":[]}',
                encoding="utf-8",
            )

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
                    app.on_data_table_row_selected(SimpleNamespace(data_table=table, cursor_row=0))
                    await pilot.pause()

                    dialog = app.screen.query_one("#rom-command-dialog")
                    button_ids = [child.id for child in dialog.children if isinstance(child, Button)]
                    recalculate_index = button_ids.index("rom-command-recalculate-crcs")
                    delete_index = button_ids.index("rom-command-delete")
                    self.assertEqual(recalculate_index + 1, delete_index)

            asyncio.run(run_assertions())

    def test_rom_command_modal_variant_a_places_run_rom_before_cancel(self) -> None:
        """No-autorun command modal places Run ROM before Create and Cancel."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)
            (rom_root / "sample.nes").write_bytes(make_ines_rom(mapper=2, submapper=0))

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
                    app.on_data_table_row_selected(SimpleNamespace(data_table=table, cursor_row=0))
                    await pilot.pause()

                    dialog = app.screen.query_one("#rom-command-dialog")
                    button_ids = [child.id for child in dialog.children if isinstance(child, Button)]
                    run_index = button_ids.index("rom-command-run-rom")
                    create_index = button_ids.index("rom-command-create")
                    cancel_index = button_ids.index("rom-command-cancel")
                    self.assertEqual(run_index + 1, create_index)
                    self.assertEqual(create_index + 1, cancel_index)

            asyncio.run(run_assertions())

    def test_rom_command_modal_variant_b_places_run_rom_first_with_autorun(self) -> None:
        """Autorun command modal places Run ROM first, followed by Playback headless."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)
            (rom_root / "sample.nes").write_bytes(make_ines_rom(mapper=2, submapper=0))
            (rom_root / "sample.autorun").write_text(
                '{"version":3,"frames":[{"player1":0,"player2":0,"repeat":1}],"checkpoints":[]}',
                encoding="utf-8",
            )

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
                    app.on_data_table_row_selected(SimpleNamespace(data_table=table, cursor_row=0))
                    await pilot.pause()

                    dialog = app.screen.query_one("#rom-command-dialog")
                    button_ids = [child.id for child in dialog.children if isinstance(child, Button)]
                    self.assertEqual(button_ids[0], "rom-command-run-rom")
                    self.assertEqual(button_ids[1], "rom-command-playback-headless")

            asyncio.run(run_assertions())

    def test_autorun_column_shows_not_run_in_grey_when_autorun_exists(self) -> None:
        """Autorun column shows grey Not run when sibling .autorun exists but no playback result."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)

            (rom_root / "sample.nes").write_bytes(make_ines_rom(mapper=2, submapper=0))
            (rom_root / "sample.autorun").write_text("{}", encoding="utf-8")

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
                    status_cell = table.get_row_at(0)[3]
                    self.assertIsInstance(status_cell, Text)
                    self.assertEqual(status_cell.plain, "Not run")
                    self.assertIn("grey", str(status_cell.style))

            asyncio.run(run_assertions())

    def test_autorun_column_shows_pass_in_green_when_last_playback_passed(self) -> None:
        """Autorun column shows PASS in green when persisted status is passed."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)

            (rom_root / "sample.nes").write_bytes(make_ines_rom(mapper=2, submapper=0))
            (rom_root / "sample.autorun").write_text("{}", encoding="utf-8")

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
                    app._set_record_autorun_status("sample.nes", "passed")
                    table = app.query_one("#rom-database", DataTable)
                    status_cell = table.get_row_at(0)[3]
                    self.assertIsInstance(status_cell, Text)
                    self.assertEqual(status_cell.plain, "PASS")
                    self.assertIn("green", str(status_cell.style))

            asyncio.run(run_assertions())

    def test_autorun_column_shows_fail_in_red_when_last_playback_failed(self) -> None:
        """Autorun column shows FAIL in red when persisted status is failed."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)

            (rom_root / "sample.nes").write_bytes(make_ines_rom(mapper=2, submapper=0))
            (rom_root / "sample.autorun").write_text("{}", encoding="utf-8")

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
                    app._set_record_autorun_status("sample.nes", "failed")
                    table = app.query_one("#rom-database", DataTable)
                    status_cell = table.get_row_at(0)[3]
                    self.assertIsInstance(status_cell, Text)
                    self.assertEqual(status_cell.plain, "FAIL")
                    self.assertIn("red", str(status_cell.style))

            asyncio.run(run_assertions())

    def test_autorun_column_shows_na_without_autorun_file(self) -> None:
        """Autorun column shows N/A with no highlight when no autorun file exists."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)

            (rom_root / "sample.nes").write_bytes(make_ines_rom(mapper=2, submapper=0))

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
                    status_cell = table.get_row_at(0)[3]
                    self.assertEqual(status_cell, "N/A")

            asyncio.run(run_assertions())

    def test_rom_root_input_value_persists_between_runs(self) -> None:
        """Editing ROM root input persists value and next app run loads it."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            settings_path = temp_root / "mappertool_settings.json"
            rom_db_path = temp_root / "rom_db.csv"
            rom_db_path.write_text("# empty\n", encoding="utf-8")
            rom_files_db_path = temp_root / "rom_files.csv"

            new_rom_root = temp_root / "custom_roms"

            async def run_assertions() -> None:
                app = MapperToolApp(
                    rom_db_csv_path=rom_db_path,
                    rom_files_csv_path=rom_files_db_path,
                    settings_path=settings_path,
                )
                async with app.run_test() as pilot:
                    await pilot.pause()
                    rom_root_input = app.query_one("#rom-root-input", Input)
                    rom_root_input.value = str(new_rom_root)
                    app.on_input_changed(SimpleNamespace(input=rom_root_input, value=str(new_rom_root)))

                app_restarted = MapperToolApp(
                    rom_db_csv_path=rom_db_path,
                    rom_files_csv_path=rom_files_db_path,
                    settings_path=settings_path,
                )
                self.assertEqual(app_restarted.rom_root, new_rom_root)

            asyncio.run(run_assertions())

    def test_filter_values_persist_between_runs(self) -> None:
        """Mapper/name/autorun filters are persisted and loaded at next startup."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            settings_path = temp_root / "mappertool_settings.json"
            rom_db_path = temp_root / "rom_db.csv"
            rom_db_path.write_text("# empty\n", encoding="utf-8")
            rom_files_db_path = temp_root / "rom_files.csv"

            async def run_assertions() -> None:
                app = MapperToolApp(
                    rom_db_csv_path=rom_db_path,
                    rom_files_csv_path=rom_files_db_path,
                    settings_path=settings_path,
                )
                async with app.run_test() as pilot:
                    await pilot.pause()
                    mapper_filter = app.query_one("#mapper-filter-input", Input)
                    name_filter = app.query_one("#name-filter-input", Input)
                    autorun_filter = app.query_one("#autorun-only-filter", Checkbox)

                    mapper_filter.value = "24,25"
                    app.on_input_changed(SimpleNamespace(input=mapper_filter, value="24,25"))
                    name_filter.value = "mario"
                    app.on_input_changed(SimpleNamespace(input=name_filter, value="mario"))
                    app.on_checkbox_changed(SimpleNamespace(checkbox=autorun_filter, value=True))

                app_restarted = MapperToolApp(
                    rom_db_csv_path=rom_db_path,
                    rom_files_csv_path=rom_files_db_path,
                    settings_path=settings_path,
                )

                self.assertEqual(app_restarted._rom_mapper_filter_text, "24,25")
                self.assertEqual(app_restarted._rom_name_filter, "mario")
                self.assertTrue(app_restarted._show_only_autorun)

                async with app_restarted.run_test() as pilot:
                    await pilot.pause()
                    self.assertEqual(
                        app_restarted.query_one("#mapper-filter-input", Input).value,
                        "24,25",
                    )
                    self.assertEqual(
                        app_restarted.query_one("#name-filter-input", Input).value,
                        "mario",
                    )
                    self.assertTrue(app_restarted.query_one("#autorun-only-filter", Checkbox).value)

            asyncio.run(run_assertions())

    def test_autorun_status_persists_between_application_runs(self) -> None:
        """Autorun PASS/FAIL status persists in ROM inventory between app restarts."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)
            (rom_root / "sample.nes").write_bytes(make_ines_rom(mapper=2, submapper=0))
            (rom_root / "sample.autorun").write_text("{}", encoding="utf-8")

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
                    app._set_record_autorun_status("sample.nes", "passed")

                restarted = MapperToolApp(
                    rom_db_csv_path=rom_db_path,
                    rom_root=rom_root,
                    rom_files_csv_path=rom_files_db_path,
                )
                async with restarted.run_test() as pilot:
                    await pilot.pause()
                    table = restarted.query_one("#rom-database", DataTable)
                    status_cell = table.get_row_at(0)[3]
                    self.assertIsInstance(status_cell, Text)
                    self.assertEqual(status_cell.plain, "PASS")

            asyncio.run(run_assertions())

    def test_unchanged_rom_root_input_does_not_save_settings(self) -> None:
        """ROM root input changes only persist when resolved value is actually different."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            settings_path = temp_root / "mappertool_settings.json"
            rom_db_path = temp_root / "rom_db.csv"
            rom_db_path.write_text("# empty\n", encoding="utf-8")
            rom_files_db_path = temp_root / "rom_files.csv"

            async def run_assertions() -> None:
                app = MapperToolApp(
                    rom_db_csv_path=rom_db_path,
                    rom_files_csv_path=rom_files_db_path,
                    settings_path=settings_path,
                )
                app._save_settings = Mock()  # type: ignore[method-assign]

                async with app.run_test() as pilot:
                    await pilot.pause()
                    rom_root_input = app.query_one("#rom-root-input", Input)
                    app.on_input_changed(SimpleNamespace(input=rom_root_input, value=str(app.rom_root)))
                    app._save_settings.assert_not_called()

            asyncio.run(run_assertions())

    def test_filter_changes_do_not_save_settings_with_explicit_rom_root(self) -> None:
        """Filter edits avoid config writes when app uses explicit ROM root without explicit settings path."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)
            (rom_root / "demo.nes").write_bytes(make_ines_rom(mapper=2, submapper=0))

            rom_db_path = temp_root / "rom_db.csv"
            rom_db_path.write_text("# empty\n", encoding="utf-8")
            rom_files_db_path = temp_root / "rom_files.csv"

            async def run_assertions() -> None:
                app = MapperToolApp(
                    rom_db_csv_path=rom_db_path,
                    rom_root=rom_root,
                    rom_files_csv_path=rom_files_db_path,
                )
                app._save_settings = Mock()  # type: ignore[method-assign]

                async with app.run_test() as pilot:
                    await pilot.pause()
                    mapper_filter = app.query_one("#mapper-filter-input", Input)
                    name_filter = app.query_one("#name-filter-input", Input)
                    autorun_filter = app.query_one("#autorun-only-filter", Checkbox)

                    app.on_input_changed(SimpleNamespace(input=mapper_filter, value="24"))
                    app.on_input_changed(SimpleNamespace(input=name_filter, value="demo"))
                    app.on_checkbox_changed(SimpleNamespace(checkbox=autorun_filter, value=True))

                    app._save_settings.assert_not_called()

            asyncio.run(run_assertions())

    def test_drop_and_rescan_rebuilds_inventory_from_current_rom_root(self) -> None:
        """Drop and rescan clears prior inventory and rebuilds from disk."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)
            rom_db_path = temp_root / "rom_db.csv"
            rom_db_path.write_text("# empty\n", encoding="utf-8")
            rom_files_db_path = temp_root / "rom_files.csv"

            first_rom = rom_root / "first.nes"
            first_rom.write_bytes(make_ines_rom(mapper=1, submapper=0))

            async def run_assertions() -> None:
                app = MapperToolApp(
                    rom_db_csv_path=rom_db_path,
                    rom_root=rom_root,
                    rom_files_csv_path=rom_files_db_path,
                )
                async with app.run_test() as pilot:
                    await pilot.pause()
                    self.assertIn("first.nes", app.rom_file_records)

                    first_rom.unlink()
                    (rom_root / "second.nes").write_bytes(make_ines_rom(mapper=2, submapper=0))

                    app.on_button_pressed(SimpleNamespace(button=SimpleNamespace(id="drop-rescan-button")))
                    await pilot.pause()
                    await pilot.pause()

                    self.assertNotIn("first.nes", app.rom_file_records)
                    self.assertIn("second.nes", app.rom_file_records)

            asyncio.run(run_assertions())

    def test_initial_scan_and_rescan_show_completion_modal(self) -> None:
        """Initial scan and drop-rescan each push scan-progress modal dialog."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)
            (rom_root / "demo.nes").write_bytes(make_ines_rom(mapper=1, submapper=0))

            rom_db_path = temp_root / "rom_db.csv"
            rom_db_path.write_text("# empty\n", encoding="utf-8")
            rom_files_db_path = temp_root / "rom_files.csv"

            async def run_assertions() -> None:
                app = MapperToolApp(
                    rom_db_csv_path=rom_db_path,
                    rom_root=rom_root,
                    rom_files_csv_path=rom_files_db_path,
                )
                pushed: list[object] = []
                app.push_screen = lambda screen, callback=None: pushed.append(screen)  # type: ignore[method-assign]

                async with app.run_test() as pilot:
                    await pilot.pause()
                    self.assertTrue(pushed)
                    self.assertEqual(type(pushed[0]).__name__, "ScanProgressModal")

                    app.on_button_pressed(SimpleNamespace(button=SimpleNamespace(id="drop-rescan-button")))
                    await pilot.pause()
                    self.assertGreaterEqual(len(pushed), 2)
                    self.assertEqual(type(pushed[1]).__name__, "ScanProgressModal")

            asyncio.run(run_assertions())

    def test_request_scan_cancel_sets_cancel_flag_when_scan_running(self) -> None:
        """Cancel request marks running scan cancellation event."""

        app = MapperToolApp()
        app._scan_in_progress = True
        app.request_scan_cancel()
        self.assertTrue(app._scan_cancel_event.is_set())

    def test_playback_all_button_starts_batch_playback_worker(self) -> None:
        """Playback All button dispatches a dedicated batch playback worker."""

        app = MapperToolApp()
        run_worker_mock = _install_run_worker_mock(app)

        app.on_button_pressed(SimpleNamespace(button=SimpleNamespace(id="playback-all-button")))

        run_worker_mock.assert_called_once()
        call_kwargs = run_worker_mock.call_args.kwargs
        self.assertEqual(call_kwargs["group"], "playback-all")
        self.assertTrue(call_kwargs["exclusive"])

    def test_playback_not_run_button_starts_batch_playback_worker(self) -> None:
        """Playback Not run button dispatches a dedicated filtered playback worker."""

        app = MapperToolApp()
        run_worker_mock = _install_run_worker_mock(app)

        app.on_button_pressed(SimpleNamespace(button=SimpleNamespace(id="playback-not-run-button")))

        run_worker_mock.assert_called_once()
        call_kwargs = run_worker_mock.call_args.kwargs
        self.assertEqual(call_kwargs["group"], "playback-not-run")
        self.assertTrue(call_kwargs["exclusive"])

    def test_recalculate_failing_crcs_button_starts_batch_worker(self) -> None:
        """Recalculate failing CRCs button dispatches dedicated batch worker."""

        app = MapperToolApp()
        run_worker_mock = _install_run_worker_mock(app)

        app.on_button_pressed(SimpleNamespace(button=SimpleNamespace(id="recalculate-failing-crcs-button")))

        run_worker_mock.assert_called_once()
        call_kwargs = run_worker_mock.call_args.kwargs
        self.assertEqual(call_kwargs["group"], "recalculate-failing-crcs")
        self.assertTrue(call_kwargs["exclusive"])

    def test_playback_all_runs_each_autorun_with_file_progress_context(self) -> None:
        """Playback All runs all autorun files and passes file progress context X/Y."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)

            (rom_root / "first.nes").write_bytes(make_ines_rom(mapper=1, submapper=0))
            (rom_root / "first.autorun").write_text("{}", encoding="utf-8")
            (rom_root / "second.nes").write_bytes(make_ines_rom(mapper=2, submapper=0))
            (rom_root / "second.autorun").write_text("{}", encoding="utf-8")
            (rom_root / "third.nes").write_bytes(make_ines_rom(mapper=3, submapper=0))

            rom_db_path = temp_root / "rom_db.csv"
            rom_db_path.write_text("# empty\n", encoding="utf-8")
            rom_files_db_path = temp_root / "rom_files.csv"

            async def run_assertions() -> None:
                app = MapperToolApp(
                    rom_db_csv_path=rom_db_path,
                    rom_root=rom_root,
                    rom_files_csv_path=rom_files_db_path,
                )
                captured_calls: list[tuple[list[str], str, str, str]] = []

                async def fake_run(
                    command: list[str],
                    command_id: str,
                    full_set_progress_status: str = "",
                    current_file_progress_status: str = "",
                ) -> str | None:
                    captured_calls.append(
                        (
                            command,
                            command_id,
                            full_set_progress_status,
                            current_file_progress_status,
                        )
                    )
                    return "passed"

                app._run_autorun_command_with_status_modal = fake_run  # type: ignore[method-assign]

                async with app.run_test() as pilot:
                    await pilot.pause()
                    await app._playback_all_autorun_files()

                    self.assertEqual(len(captured_calls), 2)
                    self.assertEqual(captured_calls[0][1], "rom-command-playback-headless")
                    self.assertEqual(captured_calls[0][2], "File 1/2: first.nes")
                    self.assertEqual(captured_calls[1][2], "File 2/2: second.nes")
                    self.assertEqual(captured_calls[0][3], "Checkpoint: -")
                    self.assertIn("--playback-headless", captured_calls[0][0])
                    self.assertEqual(app.rom_file_records["first.nes"].autorun_status, "passed")
                    self.assertEqual(app.rom_file_records["second.nes"].autorun_status, "passed")

            asyncio.run(run_assertions())

    def test_playback_not_run_runs_only_not_run_autorun_files(self) -> None:
        """Playback Not run runs only autorun records currently marked as not_run."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)

            (rom_root / "first.nes").write_bytes(make_ines_rom(mapper=1, submapper=0))
            (rom_root / "first.autorun").write_text("{}", encoding="utf-8")
            (rom_root / "second.nes").write_bytes(make_ines_rom(mapper=2, submapper=0))
            (rom_root / "second.autorun").write_text("{}", encoding="utf-8")
            (rom_root / "third.nes").write_bytes(make_ines_rom(mapper=3, submapper=0))
            (rom_root / "third.autorun").write_text("{}", encoding="utf-8")

            rom_db_path = temp_root / "rom_db.csv"
            rom_db_path.write_text("# empty\n", encoding="utf-8")
            rom_files_db_path = temp_root / "rom_files.csv"

            async def run_assertions() -> None:
                app = MapperToolApp(
                    rom_db_csv_path=rom_db_path,
                    rom_root=rom_root,
                    rom_files_csv_path=rom_files_db_path,
                )
                captured_calls: list[tuple[list[str], str, str, str]] = []

                async def fake_run(
                    command: list[str],
                    command_id: str,
                    full_set_progress_status: str = "",
                    current_file_progress_status: str = "",
                ) -> str | None:
                    captured_calls.append(
                        (
                            command,
                            command_id,
                            full_set_progress_status,
                            current_file_progress_status,
                        )
                    )
                    return "passed"

                app._run_autorun_command_with_status_modal = fake_run  # type: ignore[method-assign]

                async with app.run_test() as pilot:
                    await pilot.pause()
                    app._set_record_autorun_status("second.nes", "passed")
                    app._set_record_autorun_status("third.nes", "failed")

                    await app._playback_not_run_autorun_files()

                    self.assertEqual(len(captured_calls), 1)
                    self.assertEqual(captured_calls[0][2], "File 1/1: first.nes")
                    self.assertIn("first.nes", captured_calls[0][0][-1])

            asyncio.run(run_assertions())

    def test_playback_not_run_can_restart_after_previous_cancel(self) -> None:
        """A stale cancel flag from previous run does not block starting a new batch playback."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)

            (rom_root / "first.nes").write_bytes(make_ines_rom(mapper=1, submapper=0))
            (rom_root / "first.autorun").write_text("{}", encoding="utf-8")

            rom_db_path = temp_root / "rom_db.csv"
            rom_db_path.write_text("# empty\n", encoding="utf-8")
            rom_files_db_path = temp_root / "rom_files.csv"

            async def run_assertions() -> None:
                app = MapperToolApp(
                    rom_db_csv_path=rom_db_path,
                    rom_root=rom_root,
                    rom_files_csv_path=rom_files_db_path,
                )
                call_count = 0

                async def fake_run(
                    command: list[str],
                    command_id: str,
                    full_set_progress_status: str = "",
                    current_file_progress_status: str = "",
                ) -> str | None:
                    nonlocal call_count
                    call_count += 1
                    return "passed"

                app._run_autorun_command_with_status_modal = fake_run  # type: ignore[method-assign]

                async with app.run_test() as pilot:
                    await pilot.pause()
                    app._autorun_cancel_requested = True
                    await app._playback_not_run_autorun_files()
                    self.assertEqual(call_count, 1)

            asyncio.run(run_assertions())

    def test_recalculate_failing_crcs_runs_only_failed_autorun_files(self) -> None:
        """Recalculate failing CRCs runs only autorun records currently marked as failed."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)

            (rom_root / "first.nes").write_bytes(make_ines_rom(mapper=1, submapper=0))
            (rom_root / "first.autorun").write_text("{}", encoding="utf-8")
            (rom_root / "second.nes").write_bytes(make_ines_rom(mapper=2, submapper=0))
            (rom_root / "second.autorun").write_text("{}", encoding="utf-8")
            (rom_root / "third.nes").write_bytes(make_ines_rom(mapper=3, submapper=0))
            (rom_root / "third.autorun").write_text("{}", encoding="utf-8")

            rom_db_path = temp_root / "rom_db.csv"
            rom_db_path.write_text("# empty\n", encoding="utf-8")
            rom_files_db_path = temp_root / "rom_files.csv"

            async def run_assertions() -> None:
                app = MapperToolApp(
                    rom_db_csv_path=rom_db_path,
                    rom_root=rom_root,
                    rom_files_csv_path=rom_files_db_path,
                )
                captured_calls: list[tuple[list[str], str, str, str]] = []

                async def fake_run(
                    command: list[str],
                    command_id: str,
                    full_set_progress_status: str = "",
                    current_file_progress_status: str = "",
                ) -> str | None:
                    captured_calls.append(
                        (
                            command,
                            command_id,
                            full_set_progress_status,
                            current_file_progress_status,
                        )
                    )
                    return "passed"

                app._run_autorun_command_with_status_modal = fake_run  # type: ignore[method-assign]

                async with app.run_test() as pilot:
                    await pilot.pause()
                    app._set_record_autorun_status("first.nes", "failed")
                    app._set_record_autorun_status("second.nes", "passed")
                    app._set_record_autorun_status("third.nes", "failed")

                    await app._recalculate_failing_crc_autorun_files()

                    self.assertEqual(len(captured_calls), 2)
                    self.assertEqual(captured_calls[0][1], "rom-command-recalculate-crcs")
                    self.assertEqual(captured_calls[0][2], "File 1/2: first.nes")
                    self.assertEqual(captured_calls[1][2], "File 2/2: third.nes")
                    self.assertIn("--recalculate-autorun", captured_calls[0][0])
                    self.assertEqual(app.rom_file_records["first.nes"].autorun_status, "passed")
                    self.assertEqual(app.rom_file_records["third.nes"].autorun_status, "passed")

            asyncio.run(run_assertions())

    def test_playback_all_can_restart_after_previous_cancel(self) -> None:
        """Playback All also starts normally even when previous run left cancel flag set."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)

            (rom_root / "first.nes").write_bytes(make_ines_rom(mapper=1, submapper=0))
            (rom_root / "first.autorun").write_text("{}", encoding="utf-8")

            rom_db_path = temp_root / "rom_db.csv"
            rom_db_path.write_text("# empty\n", encoding="utf-8")
            rom_files_db_path = temp_root / "rom_files.csv"

            async def run_assertions() -> None:
                app = MapperToolApp(
                    rom_db_csv_path=rom_db_path,
                    rom_root=rom_root,
                    rom_files_csv_path=rom_files_db_path,
                )
                call_count = 0

                async def fake_run(
                    command: list[str],
                    command_id: str,
                    full_set_progress_status: str = "",
                    current_file_progress_status: str = "",
                ) -> str | None:
                    nonlocal call_count
                    call_count += 1
                    return "passed"

                app._run_autorun_command_with_status_modal = fake_run  # type: ignore[method-assign]

                async with app.run_test() as pilot:
                    await pilot.pause()
                    app._autorun_cancel_requested = True
                    await app._playback_all_autorun_files()
                    self.assertEqual(call_count, 1)

            asyncio.run(run_assertions())

    def test_set_autorun_modal_status_updates_three_distinct_rows(self) -> None:
        """Modal status updater forwards run/file-set/file-progress rows separately."""

        app = MapperToolApp()
        modal = Mock()
        app._autorun_run_modal = modal

        app._set_autorun_modal_status(
            "Running",
            full_set_progress_status="File 2/5: demo.nes",
            current_file_progress_status="Checkpoint 1/10. Errors: 0",
        )

        modal.set_status.assert_called_once_with(
            "Running",
            "File 2/5: demo.nes",
            "Checkpoint 1/10. Errors: 0",
        )

    def test_extract_checkpoint_progress_from_output_parses_fraction(self) -> None:
        """Checkpoint parser extracts X/Y progress from autorun output lines."""

        progress = MapperToolApp._extract_checkpoint_progress_from_output(
            "Autorun checkpoint CRC match (0x12345678) at frame 45/300, checkpoint 3/10"
        )
        self.assertEqual(progress, (3, 10))

    def test_extract_checkpoint_progress_from_output_parses_recalculate_line(self) -> None:
        """Checkpoint parser extracts X/Y from recalculation progress output."""

        progress = MapperToolApp._extract_checkpoint_progress_from_output("Recalculating checkpoint CRC(s): 7/17")
        self.assertEqual(progress, (7, 17))

    def test_request_autorun_cancel_terminates_running_process(self) -> None:
        """Canceling autorun requests subprocess termination and marks cancel state."""

        app = MapperToolApp()
        process = Mock()
        process.returncode = None
        app._autorun_subprocess = process

        app.request_autorun_cancel()

        self.assertTrue(app._autorun_cancel_requested)
        process.terminate.assert_called_once()

    # --- Rebuild before running checkbox ---

    def test_rebuild_checkbox_present_in_config_pane(self) -> None:
        """A 'Rebuild before running' checkbox exists in the Actions pane."""

        async def run_assertions() -> None:
            app = MapperToolApp()
            async with app.run_test() as pilot:
                await pilot.pause()
                checkbox = app.query_one("#rebuild-before-run-checkbox", Checkbox)
                self.assertIsNotNone(checkbox)

        asyncio.run(run_assertions())

    def test_rebuild_setting_persists_between_runs(self) -> None:
        """Rebuild before running setting round-trips through save/load."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            settings_path = temp_root / "mappertool_settings.json"
            rom_db_path = temp_root / "rom_db.csv"
            rom_db_path.write_text("# empty\n", encoding="utf-8")
            rom_files_db_path = temp_root / "rom_files.csv"

            async def run_assertions() -> None:
                app = MapperToolApp(
                    rom_db_csv_path=rom_db_path,
                    rom_files_csv_path=rom_files_db_path,
                    settings_path=settings_path,
                )
                async with app.run_test() as pilot:
                    await pilot.pause()
                    checkbox = app.query_one("#rebuild-before-run-checkbox", Checkbox)
                    app.on_checkbox_changed(SimpleNamespace(checkbox=checkbox, value=False))

                app_restarted = MapperToolApp(
                    rom_db_csv_path=rom_db_path,
                    rom_files_csv_path=rom_files_db_path,
                    settings_path=settings_path,
                )
                self.assertFalse(app_restarted._rebuild_before_run)

            asyncio.run(run_assertions())

    def test_run_rom_uses_binary_when_rebuild_unchecked(self) -> None:
        """When rebuild is unchecked, run ROM uses prebuilt binary instead of cargo."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)
            (rom_root / "sample.nes").write_bytes(make_ines_rom(mapper=2, submapper=0))

            rom_db_path = temp_root / "rom_db.csv"
            rom_db_path.write_text("# empty\n", encoding="utf-8")
            rom_files_db_path = temp_root / "rom_files.csv"

            async def run_assertions() -> None:
                app = MapperToolApp(
                    rom_db_csv_path=rom_db_path,
                    rom_root=rom_root,
                    rom_files_csv_path=rom_files_db_path,
                )
                app._rebuild_before_run = False
                captured_calls: list[tuple[list[str], str, str, str]] = []

                async def fake_run(
                    command: list[str],
                    command_id: str,
                    full_set_progress_status: str = "",
                    current_file_progress_status: str = "",
                ) -> str | None:
                    captured_calls.append((command, command_id, full_set_progress_status, current_file_progress_status))
                    return "passed"

                app._run_autorun_command_with_status_modal = fake_run  # type: ignore[method-assign]

                async with app.run_test() as pilot:
                    await pilot.pause()
                    await app._run_rom_command("sample.nes", "rom-command-run-rom")

                self.assertEqual(len(captured_calls), 1)
                command = captured_calls[0][0]
                self.assertNotIn("cargo", command)
                self.assertTrue(any("target/release/neser" in arg for arg in command))

            asyncio.run(run_assertions())

    def test_run_rom_uses_cargo_when_rebuild_checked(self) -> None:
        """When rebuild is checked (default), run ROM uses cargo run."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)
            (rom_root / "sample.nes").write_bytes(make_ines_rom(mapper=2, submapper=0))

            rom_db_path = temp_root / "rom_db.csv"
            rom_db_path.write_text("# empty\n", encoding="utf-8")
            rom_files_db_path = temp_root / "rom_files.csv"

            async def run_assertions() -> None:
                app = MapperToolApp(
                    rom_db_csv_path=rom_db_path,
                    rom_root=rom_root,
                    rom_files_csv_path=rom_files_db_path,
                )
                captured_calls: list[tuple[list[str], str, str, str]] = []

                async def fake_run(
                    command: list[str],
                    command_id: str,
                    full_set_progress_status: str = "",
                    current_file_progress_status: str = "",
                ) -> str | None:
                    captured_calls.append((command, command_id, full_set_progress_status, current_file_progress_status))
                    return "passed"

                app._run_autorun_command_with_status_modal = fake_run  # type: ignore[method-assign]

                async with app.run_test() as pilot:
                    await pilot.pause()
                    await app._run_rom_command("sample.nes", "rom-command-run-rom")

                self.assertEqual(len(captured_calls), 1)
                command = captured_calls[0][0]
                self.assertEqual(command[0], "cargo")

            asyncio.run(run_assertions())

    # --- Popup menu reorder ---

    def test_rom_command_modal_no_autorun_has_run_rom_first(self) -> None:
        """Without autorun, Run ROM is the first button in the command dialog."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)
            (rom_root / "sample.nes").write_bytes(make_ines_rom(mapper=2, submapper=0))

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
                    app.on_data_table_row_selected(SimpleNamespace(data_table=table, cursor_row=0))
                    await pilot.pause()

                    dialog = app.screen.query_one("#rom-command-dialog")
                    button_ids = [child.id for child in dialog.children if isinstance(child, Button)]
                    self.assertEqual(button_ids[0], "rom-command-run-rom")
                    self.assertEqual(button_ids[1], "rom-command-create")

            asyncio.run(run_assertions())

    def test_rom_command_modal_with_autorun_has_run_rom_first(self) -> None:
        """With autorun, Run ROM is the first button in the command dialog."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)
            (rom_root / "sample.nes").write_bytes(make_ines_rom(mapper=2, submapper=0))
            (rom_root / "sample.autorun").write_text(
                '{"version":3,"frames":[{"player1":0,"player2":0,"repeat":1}],"checkpoints":[]}',
                encoding="utf-8",
            )

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
                    app.on_data_table_row_selected(SimpleNamespace(data_table=table, cursor_row=0))
                    await pilot.pause()

                    dialog = app.screen.query_one("#rom-command-dialog")
                    button_ids = [child.id for child in dialog.children if isinstance(child, Button)]
                    self.assertEqual(button_ids[0], "rom-command-run-rom")

            asyncio.run(run_assertions())

    # --- Scroll position preservation ---

    def test_cursor_position_preserved_after_refresh_record_autorun_state(self) -> None:
        """Highlighted ROM row is restored after _refresh_record_autorun_state repopulates table."""

        with tempfile.TemporaryDirectory() as temp_dir_str:
            temp_root = Path(temp_dir_str)
            rom_root = temp_root / "roms"
            rom_root.mkdir(parents=True)
            (rom_root / "aaa.nes").write_bytes(make_ines_rom(mapper=1, submapper=0))
            (rom_root / "bbb.nes").write_bytes(make_ines_rom(mapper=2, submapper=0))
            (rom_root / "ccc.nes").write_bytes(make_ines_rom(mapper=3, submapper=0))

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

                    # Move cursor to the second row (bbb.nes)
                    table.move_cursor(row=1)
                    await pilot.pause()

                    # Simulate refresh after running a ROM
                    app._refresh_record_autorun_state("bbb.nes")
                    await pilot.pause()

                    # Cursor should still be on row 1 (bbb.nes)
                    self.assertEqual(table.cursor_row, 1)

            asyncio.run(run_assertions())
