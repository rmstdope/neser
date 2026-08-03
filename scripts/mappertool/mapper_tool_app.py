"""Main Textual application for mappertool."""

import asyncio
import contextlib
import json
import re
import threading
from collections.abc import Callable
from pathlib import Path
from typing import Any, ClassVar

from rich.text import Text
from textual.app import App, ComposeResult, ScreenStackError
from textual.containers import Horizontal, Vertical
from textual.coordinate import Coordinate
from textual.css.query import NoMatches
from textual.screen import ModalScreen
from textual.timer import Timer
from textual.widgets import Button, Checkbox, DataTable, Input, Label, TextArea

from .constants import REPO_ROOT
from .rom_db_entry import RomDbEntry
from .rom_db_index import RomDbIndex
from .rom_file_database import RomFileDatabase
from .rom_file_record import RomFileRecord


class MapperToolApp(App[None]):
    """Initial full-screen layout for autorun tooling."""

    TITLE = "Neser Mappertool"
    BINDINGS: ClassVar[list[tuple[str, str, str]]] = [
        ("meta+c", "copy_log_selection", "Copy log selection"),
        ("ctrl+shift+c", "copy_log_selection", "Copy log selection"),
    ]
    DEFAULT_ROM_DB_PATH = Path("src/cartridge/rom_db.csv")
    DEFAULT_ROM_ROOT = Path("roms/games/collection")
    DEFAULT_ROM_FILES_DB_PATH = Path("scripts/mappertool/rom_files.csv")
    DEFAULT_SETTINGS_PATH = Path("scripts/mappertool/mappertool_settings.json")
    ROM_NAME_MAX_DISPLAY_CHARS = 30
    ROM_NAME_SCROLL_START_DELAY_SECONDS = 2.0
    ROM_NAME_SCROLL_TICK_SECONDS = 0.25
    ROM_PANE_TITLE = "ROMs"
    CONFIG_PANE_TITLE = "Actions"
    LOGS_PANE_TITLE = "Logs"
    _CHECKPOINT_PROGRESS_RE = re.compile(
        r"(?:checkpoint|recalculating\s+checkpoint\s+crc\(s\):)\s*(\d+)\s*/\s*(\d+)",
        re.IGNORECASE,
    )
    CSS = """
    Screen {
        layout: vertical;
    }

    #top-panes {
        layout: horizontal;
        height: 1fr;
    }

    #bottom-panes {
        layout: horizontal;
        height: 12;
    }

    .pane {
        width: 1fr;
        border: solid $accent;
        padding: 0 1;
        overflow: hidden;
    }

    #rom-pane {
        layout: vertical;
        width: 3fr;
    }

    #rom-filters {
        layout: horizontal;
        height: auto;
        margin-bottom: 1;
    }

    #mapper-filter-input,
    #name-filter-input {
        height: 3;
        width: 1fr;
    }

    #mapper-filter-input {
        margin-right: 1;
    }

    #autorun-only-filter {
        height: auto;
        margin-bottom: 1;
    }

    #rom-database {
        height: 1fr;
        min-height: 0;
    }

    #config-editor {
        layout: vertical;
        width: 2fr;
    }

    #logs-pane {
        layout: vertical;
    }

    #logs {
        width: 1fr;
        height: 1fr;
    }

    #rom-inventory-section {
        layout: vertical;
    }

    #rom-root-input {
        height: 3;
        margin-bottom: 1;
    }

    #drop-rescan-button,
    #playback-all-button,
    #playback-not-run-button,
    #recalculate-failing-crcs-button {
        height: auto;
    }

    ScanProgressModal {
        align: center middle;
    }

    #scan-progress-dialog {
        width: 52;
        border: round $accent;
        background: $surface;
        padding: 1 2;
        height: auto;
    }

    #scan-cancel-button {
        width: 100%;
        margin-top: 1;
    }

    AutorunRunModal {
        align: center middle;
    }

    #autorun-run-dialog {
        width: 62;
        border: round $accent;
        background: $surface;
        padding: 1 2;
        height: auto;
    }

    #autorun-run-cancel-button {
        width: 100%;
        margin-top: 1;
    }

    RomCommandModal {
        align: center middle;
    }

    #rom-command-dialog {
        width: 62;
        border: round $accent;
        background: $surface;
        padding: 1 2;
        height: auto;
    }

    .rom-command-button {
        width: 100%;
        margin-top: 1;
    }
    """

    class ScanProgressModal(ModalScreen[None]):
        """Modal shown while scan is running."""

        BINDINGS: ClassVar[list[tuple[str, str, str]]] = [
            ("escape", "cancel_scan", "Cancel"),
        ]

        def __init__(self, message: str) -> None:
            super().__init__()
            self.message = message

        def compose(self) -> ComposeResult:
            with Vertical(id="scan-progress-dialog"):
                yield Label(self.message, id="scan-progress-message")
                yield Button("Cancel", id="scan-cancel-button", variant="warning")

        def on_button_pressed(self, event: Button.Pressed) -> None:
            if event.button.id == "scan-cancel-button":
                typed_app = self.app
                typed_app.request_scan_cancel()

        def action_cancel_scan(self) -> None:
            typed_app = self.app
            typed_app.request_scan_cancel()

    class RomCommandModal(ModalScreen[None]):
        """Modal showing available commands for selected ROM."""

        BINDINGS: ClassVar[list[tuple[str, str, str]]] = [
            ("escape", "dismiss", "Close"),
        ]

        def __init__(self, rom_path: str, has_autorun: bool, autorun_summary: str | None = None) -> None:
            super().__init__()
            self.rom_path = rom_path
            self.has_autorun = has_autorun
            self.autorun_summary = autorun_summary

        def compose(self) -> ComposeResult:
            rom_name = Path(self.rom_path).name
            with Vertical(id="rom-command-dialog"):
                yield Label(f"ROM Commands: {rom_name}")
                if self.autorun_summary:
                    yield Label(self.autorun_summary)
                yield Button(
                    "Run ROM",
                    id="rom-command-run-rom",
                    variant="warning",
                    classes="rom-command-button",
                )
                if self.has_autorun:
                    yield Button(
                        "Playback recording (headless)",
                        id="rom-command-playback-headless",
                        variant="warning",
                        classes="rom-command-button",
                    )
                    yield Button(
                        "Playback recording (headed)",
                        id="rom-command-playback-headed",
                        variant="warning",
                        classes="rom-command-button",
                    )
                    yield Button(
                        "Extended recording",
                        id="rom-command-extend",
                        variant="warning",
                        classes="rom-command-button",
                    )
                    yield Button(
                        "Recalculate CRCs",
                        id="rom-command-recalculate-crcs",
                        variant="warning",
                        classes="rom-command-button",
                    )
                    yield Button(
                        "Delete recording",
                        id="rom-command-delete",
                        variant="error",
                        classes="rom-command-button",
                    )
                else:
                    yield Button(
                        "Create autorun recording",
                        id="rom-command-create",
                        variant="warning",
                        classes="rom-command-button",
                    )
                yield Button(
                    "Cancel",
                    id="rom-command-cancel",
                    variant="warning",
                    classes="rom-command-button",
                )

        def on_button_pressed(self, event: Button.Pressed) -> None:
            if event.button.id == "rom-command-cancel":
                self.dismiss(None)
                return

            typed_app = self.app
            command_id = event.button.id or ""
            self.dismiss(None)
            typed_app.handle_rom_command(self.rom_path, command_id)

    class AutorunRunModal(ModalScreen[None]):
        """Modal showing autorun execution status and cancellation."""

        BINDINGS: ClassVar[list[tuple[str, str, str]]] = [
            ("escape", "cancel_autorun", "Cancel autorun"),
        ]

        def __init__(
            self,
            building_running_status: str,
            full_set_progress_status: str = "",
            current_file_progress_status: str = "",
        ) -> None:
            super().__init__()
            self.building_running_status = building_running_status
            self.full_set_progress_status = full_set_progress_status
            self.current_file_progress_status = current_file_progress_status

        def compose(self) -> ComposeResult:
            with Vertical(id="autorun-run-dialog"):
                yield Label(self.building_running_status, id="autorun-run-status-main")
                yield Label(self.full_set_progress_status, id="autorun-run-status-file-set")
                yield Label(self.current_file_progress_status, id="autorun-run-status-current-file")
                yield Button("Cancel", id="autorun-run-cancel-button", variant="warning")

        def set_status(
            self,
            building_running_status: str,
            full_set_progress_status: str | None = None,
            current_file_progress_status: str | None = None,
        ) -> None:
            try:
                self.query_one("#autorun-run-status-main", Label).update(building_running_status)
                if full_set_progress_status is not None:
                    self.query_one("#autorun-run-status-file-set", Label).update(full_set_progress_status)
                if current_file_progress_status is not None:
                    self.query_one("#autorun-run-status-current-file", Label).update(current_file_progress_status)
            except NoMatches:
                return

        def on_button_pressed(self, event: Button.Pressed) -> None:
            if event.button.id == "autorun-run-cancel-button":
                self.app.request_autorun_cancel()

        def action_cancel_autorun(self) -> None:
            self.app.request_autorun_cancel()

    def __init__(
        self,
        rom_db_csv_path: Path | None = None,
        rom_root: Path | None = None,
        rom_files_csv_path: Path | None = None,
        settings_path: Path | None = None,
    ) -> None:
        super().__init__()
        explicit_settings_path = settings_path is not None
        self.settings_path = self._resolve_repo_path(settings_path or self.DEFAULT_SETTINGS_PATH)
        self.rom_db_csv_path = self._resolve_repo_path(rom_db_csv_path or self.DEFAULT_ROM_DB_PATH)
        self._settings_persistence_enabled = explicit_settings_path or rom_root is None
        settings = self._load_settings()

        configured_rom_root = rom_root
        if configured_rom_root is None:
            raw_rom_root = settings.get("rom_root", str(self.DEFAULT_ROM_ROOT))
            configured_rom_root = Path(str(raw_rom_root))

        self.rom_root = self._resolve_repo_path(configured_rom_root)
        self.rom_files_csv_path = self._resolve_repo_path(rom_files_csv_path or self.DEFAULT_ROM_FILES_DB_PATH)
        self.rom_db_index = RomDbIndex({})
        self.rom_file_records: dict[str, RomFileRecord] = {}
        self._rom_table_full_paths: list[str] = []
        self._rom_sort_column_index: int | None = None
        self._rom_sort_reverse = False
        if rom_root is None:
            self._rom_name_filter = self._read_setting_text(settings, "rom_name_filter")
            self._rom_mapper_filter_text = self._read_setting_text(settings, "rom_mapper_filter")
            self._show_only_autorun = self._read_setting_bool(settings, "show_only_autorun")
            self._rebuild_before_run = self._read_setting_bool(settings, "rebuild_before_run", default=True)
        else:
            self._rom_name_filter = ""
            self._rom_mapper_filter_text = ""
            self._show_only_autorun = False
            self._rebuild_before_run = True
        self._rom_mapper_filter_values: set[int] = self.parse_mapper_filter_values(self._rom_mapper_filter_text)
        self._scan_cancel_event = threading.Event()
        self._scan_in_progress = False
        self._scan_modal: MapperToolApp.ScanProgressModal | None = None
        self._autorun_cancel_requested = False
        self._autorun_run_modal: MapperToolApp.AutorunRunModal | None = None
        self._autorun_subprocess: asyncio.subprocess.Process | None = None
        self._highlighted_rom_row_index: int | None = None
        self._rom_name_scroll_offset = 0
        self._rom_name_scroll_active = False
        self._rom_name_scroll_start_timer: Timer | None = None
        self._rom_name_scroll_tick_timer: Timer | None = None

    def compose(self) -> ComposeResult:
        """Build top ROM/config panes and a bottom logs pane."""

        with Horizontal(id="top-panes"):
            rom_pane = Vertical(id="rom-pane", classes="pane")
            rom_pane.border_title = self.ROM_PANE_TITLE
            with rom_pane:
                with Horizontal(id="rom-filters"):
                    yield Input(
                        placeholder="ROM name filter",
                        id="name-filter-input",
                        value=self._rom_name_filter,
                    )
                    yield Input(
                        placeholder="Mapper filter (comma separated)",
                        id="mapper-filter-input",
                        value=self._rom_mapper_filter_text,
                    )
                yield Checkbox(
                    "Show only ROMs with autorun files",
                    id="autorun-only-filter",
                    value=self._show_only_autorun,
                )
                rom_database = DataTable(id="rom-database")
                rom_database.cursor_type = "row"
                rom_database.add_columns("Map", "SMap", "ROM", "Autorun", "CRC", "Source")
                yield rom_database

            config_editor = Vertical(id="config-editor", classes="pane")
            config_editor.border_title = self.CONFIG_PANE_TITLE
            with config_editor, Vertical(id="rom-inventory-section"):
                yield Label("ROM Inventory")
                yield Input(value=str(self.rom_root), id="rom-root-input")
                yield Checkbox(
                    "Rebuild before running",
                    id="rebuild-before-run-checkbox",
                    value=self._rebuild_before_run,
                )
                yield Button("Drop and rescan", id="drop-rescan-button", variant="warning")
                yield Button(
                    "Playback All Recordings",
                    id="playback-all-button",
                    variant="warning",
                )
                yield Button(
                    "Playback Not run",
                    id="playback-not-run-button",
                    variant="warning",
                )
                yield Button(
                    "Recalculate failing CRCs",
                    id="recalculate-failing-crcs-button",
                    variant="warning",
                )

        with Horizontal(id="bottom-panes"):
            logs_pane = Vertical(id="logs-pane", classes="pane")
            logs_pane.border_title = self.LOGS_PANE_TITLE
            with logs_pane:
                logs = TextArea(id="logs", read_only=True)
                yield logs

    def on_mount(self) -> None:
        """Load databases and scan for new ROM files at startup."""

        with contextlib.suppress(NoMatches):
            self.set_focus(self.query_one("#rom-database", DataTable))

        try:
            self.rom_db_index = RomDbIndex.from_csv(self.rom_db_csv_path)
            self._append_log(f"Loaded ROM database: {self.rom_db_index.size} entries from {self.rom_db_csv_path}")
        except FileNotFoundError:
            self._append_log(f"ROM database not found: {self.rom_db_csv_path}")

        self.run_worker(self._start_scan(drop_inventory=False), group="scan", exclusive=True)

    def lookup_rom_by_crc(self, crc: int | str) -> RomDbEntry | None:
        """Return a ROM DB entry matching a full-ROM CRC value."""

        return self.rom_db_index.lookup(crc)

    def _populate_rom_table(self, rom_table: DataTable, restore_path: str | None = None) -> None:
        """Render tracked ROM records in the left-hand ROM list widget."""

        self._cancel_rom_name_scroll_timers()
        self._highlighted_rom_row_index = None
        self._rom_name_scroll_offset = 0
        self._rom_name_scroll_active = False
        rom_table.clear(columns=False)
        self._rom_table_full_paths = []
        restore_row: int | None = None
        for record in self._sorted_rom_records():
            rom_table.add_row(
                self._table_cell(record.mapper),
                self._table_cell(record.submapper),
                self._table_cell(self._rom_display_name(record.rom_path)),
                self._autorun_status_cell(record),
                self._table_cell(record.crc),
                self._table_cell(record.mapper_source),
            )
            if restore_path is not None and record.rom_path == restore_path:
                restore_row = len(self._rom_table_full_paths)
            self._rom_table_full_paths.append(record.rom_path)
        self._update_rom_table_summary(rom_table)
        if restore_row is not None:
            rom_table.move_cursor(row=restore_row)

    def _table_cell(self, value: str) -> str:
        """Return plain table cell renderable."""

        return value

    @staticmethod
    def _autorun_status_label(record: RomFileRecord) -> str:
        """Return user-facing autorun status label for ROM table."""

        if not record.has_autorun:
            return "N/A"
        if record.autorun_status == "passed":
            return "PASS"
        if record.autorun_status == "failed":
            return "FAIL"
        return "Not run"

    @classmethod
    def _autorun_status_cell(cls, record: RomFileRecord) -> str | Text:
        """Return autorun status cell with dedicated status highlighting."""

        label = cls._autorun_status_label(record)
        if label == "PASS":
            return Text(label, style="black on green")
        if label == "FAIL":
            return Text(label, style="white on red")
        if label == "Not run":
            return Text(label, style="black on grey70")
        return label

    @staticmethod
    def _rom_filename(rom_path: str) -> str:
        """Return filename-only text for a ROM path."""

        return Path(rom_path).name

    @classmethod
    def _rom_display_name(cls, rom_path: str) -> str:
        """Return filename-only display text, truncated to fit the ROM column."""

        filename = cls._rom_filename(rom_path)
        if len(filename) <= cls.ROM_NAME_MAX_DISPLAY_CHARS:
            return filename
        return filename[: cls.ROM_NAME_MAX_DISPLAY_CHARS - 3] + "..."

    def _is_highlighted_rom_name_scrollable(self, row_index: int) -> bool:
        """Return True if highlighted row has a ROM name longer than visible column budget."""

        if row_index < 0 or row_index >= len(self._rom_table_full_paths):
            return False
        rom_name = self._rom_filename(self._rom_table_full_paths[row_index])
        return len(rom_name) > self.ROM_NAME_MAX_DISPLAY_CHARS

    def _rom_display_name_for_row(self, row_index: int) -> str:
        """Return rendered ROM display name for table row, including optional scroll state."""

        rom_path = self._rom_table_full_paths[row_index]
        if (
            row_index != self._highlighted_rom_row_index
            or not self._rom_name_scroll_active
            or not self._is_highlighted_rom_name_scrollable(row_index)
        ):
            return self._rom_display_name(rom_path)

        filename = self._rom_filename(rom_path)
        source = f"{filename}   {filename}"
        cycle_length = len(filename) + 3
        start_index = self._rom_name_scroll_offset % cycle_length
        return source[start_index : start_index + self.ROM_NAME_MAX_DISPLAY_CHARS]

    def _cancel_rom_name_scroll_timers(self) -> None:
        """Cancel active highlighted-ROM scroll timers if they exist."""

        for timer in (self._rom_name_scroll_start_timer, self._rom_name_scroll_tick_timer):
            if timer is not None:
                with contextlib.suppress(Exception):
                    timer.stop()
        self._rom_name_scroll_start_timer = None
        self._rom_name_scroll_tick_timer = None

    def _update_rom_name_cell(self, row_index: int) -> None:
        """Update ROM name cell content for one row if table and row are currently valid."""

        if row_index < 0 or row_index >= len(self._rom_table_full_paths):
            return
        try:
            table = self.query_one("#rom-database", DataTable)
        except NoMatches:
            return
        if row_index >= table.row_count:
            return
        table.update_cell_at(
            Coordinate(row_index, 2),
            self._table_cell(self._rom_display_name_for_row(row_index)),
        )

    def _start_highlighted_rom_name_scroll(self) -> None:
        """Start ticker that scrolls highlighted ROM name if it is still eligible."""

        row_index = self._highlighted_rom_row_index
        if row_index is None or not self._is_highlighted_rom_name_scrollable(row_index):
            return
        self._rom_name_scroll_active = True
        self._rom_name_scroll_offset = 0
        self._update_rom_name_cell(row_index)
        self._rom_name_scroll_tick_timer = self.set_interval(
            self.ROM_NAME_SCROLL_TICK_SECONDS,
            self._advance_highlighted_rom_name_scroll,
        )

    def _advance_highlighted_rom_name_scroll(self) -> None:
        """Advance highlighted ROM name marquee by one character and refresh visible cell."""

        row_index = self._highlighted_rom_row_index
        if row_index is None or not self._rom_name_scroll_active:
            return
        if not self._is_highlighted_rom_name_scrollable(row_index):
            self._cancel_rom_name_scroll_timers()
            self._rom_name_scroll_active = False
            return

        self._rom_name_scroll_offset += 1
        self._update_rom_name_cell(row_index)

    def _update_rom_table_summary(self, rom_table: DataTable) -> None:
        """Show active sort/filter state in ROM table title."""

        summary_parts: list[str] = []
        if self._rom_sort_column_index is not None:
            summary_parts.append(
                f"Sort: {self._column_label(self._rom_sort_column_index)} {'desc' if self._rom_sort_reverse else 'asc'}"
            )
        if self._rom_name_filter.strip():
            summary_parts.append(f"Name: {self._rom_name_filter.strip()}")
        if self._rom_mapper_filter_text.strip():
            summary_parts.append(f"Mapper: {self._rom_mapper_filter_text.strip()}")
        if self._show_only_autorun:
            summary_parts.append("Autorun only")

        if summary_parts:
            rom_table.border_title = " | ".join(summary_parts)
        else:
            rom_table.border_title = "ROMs"

    @staticmethod
    def _column_label(column_index: int) -> str:
        """Return short display label for ROM table columns."""

        labels = ["Map", "SMap", "ROM", "Autorun", "CRC", "Source"]
        if 0 <= column_index < len(labels):
            return labels[column_index]
        return f"Col{column_index}"

    def _sorted_rom_records(self) -> list[RomFileRecord]:
        """Return ROM records sorted by active header-click sort option."""

        records = [
            record
            for record in self.rom_file_records.values()
            if record.is_valid and self._record_passes_filters(record)
        ]
        records.sort(key=lambda record: record.rom_path.casefold())

        if self._rom_sort_column_index is not None:
            records.sort(
                key=lambda record: self._sort_key_for_column(record, self._rom_sort_column_index or 0),
                reverse=self._rom_sort_reverse,
            )

        return records

    def _record_passes_filters(self, record: RomFileRecord) -> bool:
        """Return True when record matches active name and mapper filters."""

        name_filter = self._rom_name_filter.strip().casefold()
        if name_filter:
            rom_name = self._rom_filename(record.rom_path).casefold()
            if name_filter not in rom_name:
                return False

        if self._rom_mapper_filter_values:
            mapper_value = self._parse_mapper_number(record.mapper)
            if mapper_value is None:
                return False

            if mapper_value not in self._rom_mapper_filter_values:
                return False

        return not (self._show_only_autorun and not record.has_autorun)

    @staticmethod
    def _parse_mapper_number(value: str) -> int | None:
        """Parse mapper value as integer when possible."""

        stripped = value.strip()
        if not stripped:
            return None
        try:
            return int(stripped, 10)
        except ValueError:
            return None

    @classmethod
    def _mapper_numeric_sort_key(cls, value: str) -> tuple[int, int | str]:
        """Sort mapper values numerically when valid, blanks/non-numbers last."""

        parsed = cls._parse_mapper_number(value)
        if parsed is None:
            return (1, value.casefold())
        return (0, parsed)

    def _sort_key_for_column(self, record: RomFileRecord, column_index: int) -> tuple[int, object]:
        """Return sortable key for a given ROM table column."""

        if column_index == 0:
            return self._mapper_numeric_sort_key(record.mapper)
        if column_index == 1:
            return self._mapper_numeric_sort_key(record.submapper)
        if column_index == 2:
            return (0, self._rom_filename(record.rom_path).casefold())
        if column_index == 3:
            order = {"na": 0, "not_run": 1, "failed": 2, "passed": 3}
            return (0, order.get(record.autorun_status, 1))
        if column_index == 4:
            return (0, record.crc.casefold())
        return (0, record.mapper_source.casefold())

    @staticmethod
    def parse_mapper_filter_values(raw_text: str) -> set[int]:
        """Parse mapper filter as comma-separated exact integer values."""

        values: set[int] = set()
        for part in raw_text.split(","):
            token = part.strip()
            if not token:
                continue
            try:
                values.add(int(token, 10))
            except ValueError:
                continue
        return values

    def on_input_changed(self, event: Input.Changed) -> None:
        """Update live filters while typing in filter text fields."""

        if event.input.id == "mapper-filter-input":
            if event.value == self._rom_mapper_filter_text:
                return
            self._rom_mapper_filter_text = event.value
            self._rom_mapper_filter_values = self.parse_mapper_filter_values(event.value)
            if self._settings_persistence_enabled:
                self._save_settings()
        elif event.input.id == "name-filter-input":
            if event.value == self._rom_name_filter:
                return
            self._rom_name_filter = event.value
            if self._settings_persistence_enabled:
                self._save_settings()
        elif event.input.id == "rom-root-input":
            new_rom_root = self._resolve_repo_path(Path(event.value.strip() or str(self.DEFAULT_ROM_ROOT)))
            if new_rom_root == self.rom_root:
                return
            self.rom_root = new_rom_root
            if self._settings_persistence_enabled:
                self._save_settings()
        else:
            return

        try:
            rom_table = self.query_one("#rom-database", DataTable)
        except NoMatches:
            return
        self._populate_rom_table(rom_table)

    def on_checkbox_changed(self, event: Checkbox.Changed) -> None:
        """Update ROM table when checkbox filters change."""

        if event.checkbox.id == "rebuild-before-run-checkbox":
            self._rebuild_before_run = event.value
            if self._settings_persistence_enabled:
                self._save_settings()
            return

        if event.checkbox.id != "autorun-only-filter":
            return

        if event.value == self._show_only_autorun:
            return
        self._show_only_autorun = event.value
        if self._settings_persistence_enabled:
            self._save_settings()
        try:
            rom_table = self.query_one("#rom-database", DataTable)
        except NoMatches:
            return
        self._populate_rom_table(rom_table)

    def on_button_pressed(self, event: Button.Pressed) -> None:
        """Handle control button actions."""

        if event.button.id == "drop-rescan-button":
            self.run_worker(self._start_scan(drop_inventory=True), group="scan", exclusive=True)
            return

        if event.button.id == "playback-all-button":
            self.run_worker(
                self._playback_all_autorun_files(),
                group="playback-all",
                exclusive=True,
            )
            return

        if event.button.id == "playback-not-run-button":
            self.run_worker(
                self._playback_not_run_autorun_files(),
                group="playback-not-run",
                exclusive=True,
            )
            return

        if event.button.id == "recalculate-failing-crcs-button":
            self.run_worker(
                self._recalculate_failing_crc_autorun_files(),
                group="recalculate-failing-crcs",
                exclusive=True,
            )
            return

    def on_data_table_header_selected(self, event: DataTable.HeaderSelected) -> None:
        """Toggle sorting for the selected ROM table column header."""

        if event.data_table.id != "rom-database":
            return

        if self._rom_sort_column_index == event.column_index:
            self._rom_sort_reverse = not self._rom_sort_reverse
        else:
            self._rom_sort_column_index = event.column_index
            self._rom_sort_reverse = False

        self._populate_rom_table(event.data_table)

    def on_data_table_row_selected(self, event: DataTable.RowSelected) -> None:
        """Open ROM command dialog when a row is selected."""

        if event.data_table.id != "rom-database":
            return

        cursor_row = getattr(event, "cursor_row", -1)
        if not isinstance(cursor_row, int):
            return
        if cursor_row < 0 or cursor_row >= len(self._rom_table_full_paths):
            return

        rom_path = self._rom_table_full_paths[cursor_row]
        record = self.rom_file_records.get(rom_path)
        if record is None:
            return

        self.push_screen(
            self.RomCommandModal(
                rom_path,
                record.has_autorun,
                self._autorun_summary_for_record(record),
            )
        )

    @classmethod
    def _autorun_last_run_label(cls, status: str) -> str:
        """Return display label for persisted autorun status values."""

        normalized = status.strip().lower()
        if normalized == "passed":
            return "PASS"
        if normalized == "failed":
            return "FAIL"
        if normalized == "not_run":
            return "Not run"
        return "N/A"

    def _autorun_summary_for_record(self, record: RomFileRecord) -> str | None:
        """Build one-line autorun metadata summary for ROM command dialog."""

        if not record.has_autorun:
            return None

        autorun_path = (self.rom_root / Path(record.rom_path)).with_suffix(".autorun")
        frame_count, checkpoint_count = self._read_autorun_metadata(autorun_path)
        length_text = str(frame_count) if frame_count is not None else "unknown"
        checkpoint_text = str(checkpoint_count) if checkpoint_count is not None else "unknown"
        last_run = self._autorun_last_run_label(record.autorun_status)

        return f"Autorun: {length_text} frames, {checkpoint_text} CRCs, {last_run}"

    @staticmethod
    def _read_autorun_metadata(autorun_path: Path) -> tuple[int | None, int | None]:
        """Read frame/checkpoint counts from autorun JSON (v2/v3), tolerating invalid files."""

        try:
            raw = json.loads(autorun_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return None, None

        if not isinstance(raw, dict):
            return None, None

        checkpoints_raw = raw.get("checkpoints")
        checkpoint_count = len(checkpoints_raw) if isinstance(checkpoints_raw, list) else None

        frames_raw = raw.get("frames")
        if not isinstance(frames_raw, list):
            return None, checkpoint_count

        version_raw = raw.get("version")
        if version_raw == 3:
            frame_count = 0
            for frame in frames_raw:
                if not isinstance(frame, dict):
                    return None, checkpoint_count
                repeat_raw = frame.get("repeat")
                if not isinstance(repeat_raw, int) or repeat_raw < 0:
                    return None, checkpoint_count
                frame_count += repeat_raw
            return frame_count, checkpoint_count

        return len(frames_raw), checkpoint_count

    def handle_rom_command(self, rom_relative_path: str, command_id: str) -> None:
        """Dispatch selected ROM command from command dialog."""

        self.run_worker(
            self._run_rom_command(rom_relative_path, command_id),
            group="rom-command",
            exclusive=True,
        )

    def _neser_command_prefix(self) -> list[str]:
        """Return the command prefix for running neser, based on rebuild setting."""

        if self._rebuild_before_run:
            return ["cargo", "run", "--release", "--features", "sdl", "--bin", "neser", "--"]
        return [str(REPO_ROOT / "target" / "release" / "neser")]

    async def _run_rom_command(self, rom_relative_path: str, command_id: str) -> None:
        """Execute selected ROM command and refresh autorun marker state."""

        rom_absolute_path = self.rom_root / Path(rom_relative_path)

        if command_id == "rom-command-delete":
            autorun_path = rom_absolute_path.with_suffix(".autorun")
            try:
                if autorun_path.exists():
                    autorun_path.unlink()
                self._append_log(f"Deleted autorun recording: {autorun_path}")
            except OSError as error:
                self._append_log(f"Failed deleting autorun recording {autorun_path}: {error}")
            self._refresh_record_autorun_state(rom_relative_path)
            return

        prefix = self._neser_command_prefix()
        command: list[str] | None = None
        if command_id == "rom-command-create":
            command = [*prefix, "--create-recording"]
        elif command_id == "rom-command-extend":
            command = [*prefix, "--extend-recording"]
        elif command_id == "rom-command-playback-headed":
            command = [*prefix, "--playback"]
        elif command_id == "rom-command-playback-headless":
            command = [*prefix, "--playback-headless"]
        elif command_id == "rom-command-run-rom":
            command = list(prefix)
        elif command_id == "rom-command-recalculate-crcs":
            command = [*prefix, "--recalculate-autorun"]

        if command is None:
            self._append_log(f"Unknown ROM command: {command_id}")
            return

        command.append(str(rom_absolute_path))
        self._append_log(f"Running command: {' '.join(command)}")
        playback_result = await self._run_autorun_command_with_status_modal(command, command_id)
        if command_id in {
            "rom-command-playback-headed",
            "rom-command-playback-headless",
            "rom-command-recalculate-crcs",
        } and playback_result in {"passed", "failed"}:
            self._set_record_autorun_status(rom_relative_path, playback_result)
        self._refresh_record_autorun_state(rom_relative_path)

    async def _playback_all_autorun_files(self) -> None:
        """Playback all discovered autorun files sequentially with progress status."""

        autorun_records = self._sorted_autorun_records(
            lambda record: record.has_autorun,
        )

        await self._run_batch_autorun_for_records(
            autorun_records,
            title="Playback All",
            command_id="rom-command-playback-headless",
            command_argument="--playback-headless",
        )

    async def _playback_not_run_autorun_files(self) -> None:
        """Playback all discovered autorun files currently marked as not_run."""

        autorun_records = self._sorted_autorun_records(
            lambda record: record.has_autorun and record.autorun_status == "not_run",
        )

        await self._run_batch_autorun_for_records(
            autorun_records,
            title="Playback Not run",
            command_id="rom-command-playback-headless",
            command_argument="--playback-headless",
        )

    async def _recalculate_failing_crc_autorun_files(self) -> None:
        """Recalculate checkpoint CRCs for autorun files currently marked as failed."""

        autorun_records = self._sorted_autorun_records(
            lambda record: record.has_autorun and record.autorun_status == "failed",
        )

        await self._run_batch_autorun_for_records(
            autorun_records,
            title="Recalculate failing CRCs",
            command_id="rom-command-recalculate-crcs",
            command_argument="--recalculate-autorun",
        )

    def _sorted_autorun_records(
        self,
        predicate: Callable[[RomFileRecord], bool],
    ) -> list[RomFileRecord]:
        """Return sorted ROM records matching predicate for autorun batch operations."""

        return sorted(
            [record for record in self.rom_file_records.values() if predicate(record)],
            key=lambda record: record.rom_path.casefold(),
        )

    async def _run_batch_autorun_for_records(
        self,
        records: list[RomFileRecord],
        *,
        title: str,
        command_id: str,
        command_argument: str,
    ) -> None:
        """Run selected autorun command for ROM record set with per-file progress status."""

        self._autorun_cancel_requested = False

        if not records:
            self._append_log(f"No autorun files found for {title}")
            return

        total_records = len(records)
        self._append_log(f"{title} started for {total_records} autorun file(s)")

        for index, record in enumerate(records, start=1):
            if self._autorun_cancel_requested:
                break

            rom_absolute_path = self.rom_root / Path(record.rom_path)
            command = [*self._neser_command_prefix(), command_argument, str(rom_absolute_path)]
            file_progress = f"{index}/{total_records}"
            status_context = f"File {file_progress}: {Path(record.rom_path).name}"
            self._append_log(f"{title} running ({file_progress}): {record.rom_path}")
            playback_result = await self._run_autorun_command_with_status_modal(
                command,
                command_id,
                full_set_progress_status=status_context,
                current_file_progress_status="Checkpoint: -",
            )

            if playback_result in {"passed", "failed"}:
                self._set_record_autorun_status(record.rom_path, playback_result)
            self._refresh_record_autorun_state(record.rom_path)

            if self._autorun_cancel_requested:
                break

        if self._autorun_cancel_requested:
            self._append_log(f"{title} cancelled")
        else:
            self._append_log(f"{title} completed")

    async def _run_autorun_command_with_status_modal(
        self,
        command: list[str],
        command_id: str,
        full_set_progress_status: str = "",
        current_file_progress_status: str = "",
    ) -> str | None:
        """Run autorun command with status modal and cancellation support."""

        self._autorun_cancel_requested = False
        self._autorun_run_modal = self.AutorunRunModal(
            "Building Neser",
            full_set_progress_status,
            current_file_progress_status,
        )
        self.push_screen(self._autorun_run_modal)

        checkpoints_done = 0
        checkpoints_total = 0
        error_count = 0

        try:
            process = await asyncio.create_subprocess_exec(
                *command,
                cwd=REPO_ROOT,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
            )
            self._autorun_subprocess = process

            def process_output_line(line: str) -> None:
                nonlocal checkpoints_done, checkpoints_total, error_count
                if not line:
                    return

                self._append_log(line)

                parsed_progress = self._extract_checkpoint_progress_from_output(line)
                if parsed_progress is not None:
                    checkpoints_done, checkpoints_total = parsed_progress
                    if "MISMATCH" in line:
                        error_count += 1
                    self._set_autorun_modal_status(
                        "Running",
                        full_set_progress_status=full_set_progress_status,
                        current_file_progress_status=(
                            f"Checkpoint {checkpoints_done}/{checkpoints_total}. Errors: {error_count}"
                        ),
                    )

            async def consume_stream(reader: asyncio.StreamReader | None) -> None:
                if reader is None:
                    return

                buffer = ""

                while True:
                    chunk = await reader.read(1024)
                    if not chunk:
                        break

                    buffer += chunk.decode(errors="replace")

                    while True:
                        newline_index = buffer.find("\n")
                        carriage_index = buffer.find("\r")

                        separator_index = -1
                        if newline_index >= 0 and carriage_index >= 0:
                            separator_index = min(newline_index, carriage_index)
                        elif newline_index >= 0:
                            separator_index = newline_index
                        elif carriage_index >= 0:
                            separator_index = carriage_index

                        if separator_index < 0:
                            break

                        line = buffer[:separator_index].strip()
                        buffer = buffer[separator_index + 1 :]
                        process_output_line(line)

                process_output_line(buffer.strip())

            stdout_task = asyncio.create_task(consume_stream(process.stdout))
            stderr_task = asyncio.create_task(consume_stream(process.stderr))

            returncode = await process.wait()
            await asyncio.gather(stdout_task, stderr_task)

            if self._autorun_cancel_requested:
                self._append_log("Autorun cancelled")
                return None
            elif returncode == 0:
                self._append_log(f"ROM command completed: {command_id}")
                return "passed"
            else:
                self._append_log(f"ROM command failed ({returncode}): {command_id}")
                return "failed"
        finally:
            self._autorun_subprocess = None
            if self._autorun_run_modal is not None:
                with contextlib.suppress(Exception):
                    self._autorun_run_modal.dismiss(None)
            self._autorun_run_modal = None

    def request_autorun_cancel(self) -> None:
        """Cancel active autorun process."""

        process = self._autorun_subprocess
        if process is None or process.returncode is not None:
            return

        self._autorun_cancel_requested = True
        with contextlib.suppress(ProcessLookupError):
            process.terminate()
        with contextlib.suppress(NoMatches, ScreenStackError):
            self._append_log("Autorun cancel requested")

    def _set_autorun_modal_status(
        self,
        building_running_status: str,
        full_set_progress_status: str | None = None,
        current_file_progress_status: str | None = None,
    ) -> None:
        """Update autorun modal status text if modal is open."""

        if self._autorun_run_modal is None:
            return
        self._autorun_run_modal.set_status(
            building_running_status,
            full_set_progress_status,
            current_file_progress_status,
        )

    @classmethod
    def _extract_checkpoint_progress_from_output(cls, line: str) -> tuple[int, int] | None:
        """Extract checkpoint progress `X/Y` from output line if present."""

        match = cls._CHECKPOINT_PROGRESS_RE.search(line)
        if match is None:
            return None
        return int(match.group(1)), int(match.group(2))

    def _refresh_record_autorun_state(self, rom_relative_path: str) -> None:
        """Refresh has_autorun flag for a single tracked ROM and redraw table."""

        existing = self.rom_file_records.get(rom_relative_path)
        if existing is None:
            return

        has_autorun = (self.rom_root / Path(rom_relative_path)).with_suffix(".autorun").is_file()
        next_autorun_status = existing.autorun_status
        if not has_autorun:
            next_autorun_status = "na"
        elif existing.has_autorun is False or next_autorun_status not in {"not_run", "passed", "failed"}:
            next_autorun_status = "not_run"

        if has_autorun != existing.has_autorun or next_autorun_status != existing.autorun_status:
            self.rom_file_records[rom_relative_path] = RomFileRecord(
                rom_path=existing.rom_path,
                crc=existing.crc,
                header_mapper=existing.header_mapper,
                header_submapper=existing.header_submapper,
                mapper=existing.mapper,
                submapper=existing.submapper,
                mapper_source=existing.mapper_source,
                has_autorun=has_autorun,
                is_valid=existing.is_valid,
                parse_error=existing.parse_error,
                autorun_status=next_autorun_status,
            )
            RomFileDatabase(self.rom_files_csv_path).save(self.rom_file_records)

        try:
            rom_table = self.query_one("#rom-database", DataTable)
        except NoMatches:
            return
        self._populate_rom_table(rom_table, restore_path=rom_relative_path)

    def _set_record_autorun_status(self, rom_relative_path: str, status: str) -> None:
        """Persist autorun playback status for one ROM and redraw table."""

        existing = self.rom_file_records.get(rom_relative_path)
        if existing is None:
            return

        normalized = status.strip().lower()
        if normalized not in {"na", "not_run", "passed", "failed"}:
            return
        if normalized != "na" and not existing.has_autorun:
            return
        if normalized == existing.autorun_status:
            return

        self.rom_file_records[rom_relative_path] = RomFileRecord(
            rom_path=existing.rom_path,
            crc=existing.crc,
            header_mapper=existing.header_mapper,
            header_submapper=existing.header_submapper,
            mapper=existing.mapper,
            submapper=existing.submapper,
            mapper_source=existing.mapper_source,
            has_autorun=existing.has_autorun,
            is_valid=existing.is_valid,
            parse_error=existing.parse_error,
            autorun_status=normalized,
        )
        RomFileDatabase(self.rom_files_csv_path).save(self.rom_file_records)

        try:
            rom_table = self.query_one("#rom-database", DataTable)
        except NoMatches:
            return
        self._populate_rom_table(rom_table)

    def request_scan_cancel(self) -> None:
        """Request currently running scan to cancel."""

        if not self._scan_in_progress:
            return
        self._scan_cancel_event.set()
        with contextlib.suppress(NoMatches, ScreenStackError):
            self._append_log("Scan cancel requested")

    async def _start_scan(self, *, drop_inventory: bool) -> None:
        """Run inventory scan asynchronously with cancellable progress modal."""

        if self._scan_in_progress:
            return

        self._scan_in_progress = True
        self._scan_cancel_event = threading.Event()
        self._scan_modal = self.ScanProgressModal("Scanning ROM inventory...")
        self.push_screen(self._scan_modal)

        if drop_inventory and self.rom_files_csv_path.exists():
            self.rom_files_csv_path.unlink()
            self.rom_file_records = {}

        try:
            scan_result = await asyncio.to_thread(self._run_scan_sync)
            (
                records,
                new_records,
                updated_records,
                invalid_marked,
                warnings,
                was_cancelled,
            ) = scan_result

            self.rom_file_records = records
            for warning in warnings:
                self._append_log(warning)

            try:
                rom_table = self.query_one("#rom-database", DataTable)
                self._populate_rom_table(rom_table)
            except NoMatches:
                return

            if was_cancelled:
                self._append_log("Scan cancelled")
            else:
                self._append_log(
                    "ROM inventory: "
                    f"{len(self.rom_file_records)} known, "
                    f"{len(new_records)} new, "
                    f"{updated_records} updated, "
                    f"{invalid_marked} invalid-marked from {self.rom_root}"
                )
        finally:
            if self._scan_modal is not None:
                with contextlib.suppress(Exception):
                    self._scan_modal.dismiss(None)
            self._scan_modal = None
            self._scan_in_progress = False

    def _run_scan_sync(
        self,
    ) -> tuple[dict[str, RomFileRecord], list[RomFileRecord], int, int, list[str], bool]:
        """Perform scan work synchronously (used from worker thread)."""

        rom_file_db = RomFileDatabase(self.rom_files_csv_path)
        return rom_file_db.scan_and_update(
            self.rom_root,
            self.rom_db_index,
            should_cancel=self._scan_cancel_event.is_set,
        )

    def _load_settings(self) -> dict[str, Any]:
        """Load persisted mappertool settings."""

        if not self.settings_path.exists():
            return {}

        try:
            raw = json.loads(self.settings_path.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError):
            return {}

        if not isinstance(raw, dict):
            return {}
        return {str(key): value for key, value in raw.items()}

    def _save_settings(self) -> None:
        """Persist mappertool settings between runs."""

        self.settings_path.parent.mkdir(parents=True, exist_ok=True)
        payload = {
            "rom_root": str(self.rom_root),
            "rom_mapper_filter": self._rom_mapper_filter_text,
            "rom_name_filter": self._rom_name_filter,
            "show_only_autorun": self._show_only_autorun,
            "rebuild_before_run": self._rebuild_before_run,
        }
        self.settings_path.write_text(json.dumps(payload, indent=2), encoding="utf-8")

    @staticmethod
    def _read_setting_text(settings: dict[str, Any], key: str) -> str:
        """Read a text setting from the loaded settings payload."""

        value = settings.get(key, "")
        if isinstance(value, str):
            return value
        return str(value)

    @staticmethod
    def _read_setting_bool(settings: dict[str, Any], key: str, default: bool = False) -> bool:
        """Read a boolean setting from the loaded settings payload."""

        value = settings.get(key, default)
        if isinstance(value, bool):
            return value
        if isinstance(value, str):
            return value.strip().lower() in {"1", "true", "yes", "on"}
        if isinstance(value, (int, float)):
            return value != 0
        return default

    def on_data_table_row_highlighted(self, event: DataTable.RowHighlighted) -> None:
        """Show full ROM path tooltip when hovering/highlighting a ROM table row."""

        if event.data_table.id != "rom-database":
            return

        previous_row = self._highlighted_rom_row_index
        was_scrolling = self._rom_name_scroll_active
        self._cancel_rom_name_scroll_timers()
        self._rom_name_scroll_active = False
        self._rom_name_scroll_offset = 0

        if 0 <= event.cursor_row < len(self._rom_table_full_paths):
            self._highlighted_rom_row_index = event.cursor_row
            event.data_table.tooltip = self._rom_table_full_paths[event.cursor_row]

            if was_scrolling and previous_row is not None and previous_row != event.cursor_row:
                self._update_rom_name_cell(previous_row)

            if self._is_highlighted_rom_name_scrollable(event.cursor_row):
                self._rom_name_scroll_start_timer = self.set_timer(
                    self.ROM_NAME_SCROLL_START_DELAY_SECONDS,
                    self._start_highlighted_rom_name_scroll,
                )

            self.call_after_refresh(event.data_table._scroll_cursor_into_view, animate=False)
            return

        self._highlighted_rom_row_index = None
        if was_scrolling and previous_row is not None:
            self._update_rom_name_cell(previous_row)
        event.data_table.tooltip = None

    def _append_log(self, message: str) -> None:
        """Append one line to the selectable/copyable log textarea."""

        logs = self.query_one("#logs", TextArea)
        logs.insert(f"{message}\n")

    def action_copy_log_selection(self) -> None:
        """Copy current log selection to the system clipboard."""

        logs = self.query_one("#logs", TextArea)
        selected_text = logs.selected_text
        if not selected_text:
            return

        self.copy_to_clipboard(selected_text)
        self._append_log("Copied selected log text to clipboard")

    @staticmethod
    def _resolve_repo_path(path: Path) -> Path:
        """Resolve relative paths against repository root."""

        if path.is_absolute():
            return path
        return REPO_ROOT / path
