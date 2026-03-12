"""Main Textual application for mappertool."""

import asyncio
import contextlib
import json
from pathlib import Path
import re
import threading
from typing import Any

from rich.text import Text
from textual.app import App, ComposeResult, ScreenStackError
from textual.css.query import NoMatches
from textual.containers import Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Checkbox, DataTable, Input, Label, TextArea

from .constants import REPO_ROOT
from .rom_db_entry import RomDbEntry
from .rom_db_index import RomDbIndex
from .rom_file_database import RomFileDatabase
from .rom_file_record import RomFileRecord


class MapperToolApp(App[None]):
    """Initial full-screen layout for autorun tooling."""

    TITLE = "Neser Mappertool"
    BINDINGS = [
        ("meta+c", "copy_log_selection", "Copy log selection"),
        ("ctrl+shift+c", "copy_log_selection", "Copy log selection"),
    ]
    DEFAULT_ROM_DB_PATH = Path("src/cartridge/rom_db.csv")
    DEFAULT_ROM_ROOT = Path("roms/games/collection")
    DEFAULT_ROM_FILES_DB_PATH = Path("scripts/mappertool/rom_files.csv")
    DEFAULT_SETTINGS_PATH = Path("scripts/mappertool/mappertool_settings.json")
    ROM_NAME_MAX_DISPLAY_CHARS = 30
    ROM_PANE_TITLE = "ROMs"
    CONFIG_PANE_TITLE = "Actions"
    LOGS_PANE_TITLE = "Logs"
    _CHECKPOINT_PROGRESS_RE = re.compile(r"checkpoint\s+(\d+)\s*/\s*(\d+)", re.IGNORECASE)
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
    #playback-all-button {
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

        BINDINGS = [
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

        BINDINGS = [
            ("escape", "dismiss", "Close"),
        ]

        def __init__(self, rom_path: str, has_autorun: bool) -> None:
            super().__init__()
            self.rom_path = rom_path
            self.has_autorun = has_autorun

        def compose(self) -> ComposeResult:
            rom_name = Path(self.rom_path).name
            with Vertical(id="rom-command-dialog"):
                yield Label(f"ROM Commands: {rom_name}")
                if self.has_autorun:
                    yield Button(
                        "Playback recording (headless)",
                        id="rom-command-playback-headless",
                        classes="rom-command-button",
                    )
                    yield Button(
                        "Playback recording (headed)",
                        id="rom-command-playback-headed",
                        classes="rom-command-button",
                    )
                    yield Button(
                        "Etended recording",
                        id="rom-command-extend",
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
                        classes="rom-command-button",
                    )
                yield Button("Cancel", id="rom-command-cancel", classes="rom-command-button")

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

        BINDINGS = [
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
                    self.query_one("#autorun-run-status-current-file", Label).update(
                        current_file_progress_status
                    )
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
        self.rom_files_csv_path = self._resolve_repo_path(
            rom_files_csv_path or self.DEFAULT_ROM_FILES_DB_PATH
        )
        self.rom_db_index = RomDbIndex({})
        self.rom_file_records: dict[str, RomFileRecord] = {}
        self._rom_table_full_paths: list[str] = []
        self._rom_sort_column_index: int | None = None
        self._rom_sort_reverse = False
        if rom_root is None:
            self._rom_name_filter = self._read_setting_text(settings, "rom_name_filter")
            self._rom_mapper_filter_text = self._read_setting_text(settings, "rom_mapper_filter")
            self._show_only_autorun = self._read_setting_bool(settings, "show_only_autorun")
        else:
            self._rom_name_filter = ""
            self._rom_mapper_filter_text = ""
            self._show_only_autorun = False
        self._rom_mapper_filter_values: set[int] = self.parse_mapper_filter_values(
            self._rom_mapper_filter_text
        )
        self._scan_cancel_event = threading.Event()
        self._scan_in_progress = False
        self._scan_modal: MapperToolApp.ScanProgressModal | None = None
        self._autorun_cancel_requested = False
        self._autorun_run_modal: MapperToolApp.AutorunRunModal | None = None
        self._autorun_subprocess: asyncio.subprocess.Process | None = None
        self._autorun_playback_results: dict[str, str] = {}

    def compose(self) -> ComposeResult:
        """Build top ROM/config panes and a bottom logs pane."""

        with Horizontal(id="top-panes"):
            rom_pane = Vertical(id="rom-pane", classes="pane")
            rom_pane.border_title = self.ROM_PANE_TITLE
            with rom_pane:
                with Horizontal(id="rom-filters"):
                    yield Input(
                        placeholder="Mapper filter (comma separated)",
                        id="mapper-filter-input",
                        value=self._rom_mapper_filter_text,
                    )
                    yield Input(
                        placeholder="ROM name filter",
                        id="name-filter-input",
                        value=self._rom_name_filter,
                    )
                yield Checkbox(
                    "Show only ROMs with autorun files",
                    id="autorun-only-filter",
                    value=self._show_only_autorun,
                )
                rom_database = DataTable(id="rom-database")
                rom_database.cursor_type = "row"
                rom_database.add_columns("Map", "SMap", "ROM", "CRC", "Source")
                yield rom_database

            config_editor = Vertical(id="config-editor", classes="pane")
            config_editor.border_title = self.CONFIG_PANE_TITLE
            with config_editor:
                with Vertical(id="rom-inventory-section"):
                    yield Label("ROM Inventory")
                    yield Input(value=str(self.rom_root), id="rom-root-input")
                    yield Button("Drop and rescan", id="drop-rescan-button", variant="warning")
                    yield Button("Playback All", id="playback-all-button", variant="warning")

        with Horizontal(id="bottom-panes"):
            logs_pane = Vertical(id="logs-pane", classes="pane")
            logs_pane.border_title = self.LOGS_PANE_TITLE
            with logs_pane:
                logs = TextArea(id="logs", read_only=True)
                logs.load_text("Mappertool logs will appear here.\n")
                yield logs

    def on_mount(self) -> None:
        """Load databases and scan for new ROM files at startup."""

        try:
            self.rom_db_index = RomDbIndex.from_csv(self.rom_db_csv_path)
            self._append_log(
                f"Loaded ROM database: {self.rom_db_index.size} entries from {self.rom_db_csv_path}"
            )
        except FileNotFoundError:
            self._append_log(f"ROM database not found: {self.rom_db_csv_path}")

        self.run_worker(self._start_scan(drop_inventory=False), group="scan", exclusive=True)

    def lookup_rom_by_crc(self, crc: int | str) -> RomDbEntry | None:
        """Return a ROM DB entry matching a full-ROM CRC value."""

        return self.rom_db_index.lookup(crc)

    def _populate_rom_table(self, rom_table: DataTable) -> None:
        """Render tracked ROM records in the left-hand ROM list widget."""

        rom_table.clear(columns=False)
        self._rom_table_full_paths = []
        for record in self._sorted_rom_records():
            rom_table.add_row(
                self._table_cell(record.mapper, record),
                self._table_cell(record.submapper, record),
                self._table_cell(self._rom_display_name(record.rom_path), record),
                self._table_cell(record.crc, record),
                self._table_cell(record.mapper_source, record),
            )
            self._rom_table_full_paths.append(record.rom_path)
        self._update_rom_table_summary(rom_table)

    def _table_cell(self, value: str, record: RomFileRecord) -> str | Text:
        """Return table cell renderable, highlighted for ROMs with autorun files."""

        if not record.has_autorun:
            return value

        playback_result = self._autorun_playback_results.get(record.rom_path)
        if playback_result == "passed":
            return Text(value, style="black on green")
        if playback_result == "failed":
            return Text(value, style="white on red")
        return Text(value, style="black on yellow")

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

    def _update_rom_table_summary(self, rom_table: DataTable) -> None:
        """Show active sort/filter state in ROM table title."""

        summary_parts: list[str] = []
        if self._rom_sort_column_index is not None:
            summary_parts.append(
                f"Sort: {self._column_label(self._rom_sort_column_index)}"
                f" {'desc' if self._rom_sort_reverse else 'asc'}"
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

        labels = ["Map", "SMap", "ROM", "CRC", "Source"]
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

        if self._show_only_autorun and not record.has_autorun:
            return False

        return True

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
            new_rom_root = self._resolve_repo_path(
                Path(event.value.strip() or str(self.DEFAULT_ROM_ROOT))
            )
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

        self.push_screen(self.RomCommandModal(rom_path, record.has_autorun))

    def handle_rom_command(self, rom_relative_path: str, command_id: str) -> None:
        """Dispatch selected ROM command from command dialog."""

        self.run_worker(
            self._run_rom_command(rom_relative_path, command_id),
            group="rom-command",
            exclusive=True,
        )

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

        command: list[str] | None = None
        if command_id == "rom-command-create":
            command = ["cargo", "run", "--release", "--features", "sdl", "--bin", "neser", "--", "--create-recording"]
        elif command_id == "rom-command-extend":
            command = ["cargo", "run", "--release", "--features", "sdl", "--bin", "neser", "--", "--extend-recording"]
        elif command_id == "rom-command-playback-headed":
            command = ["cargo", "run", "--release", "--features", "sdl", "--bin", "neser", "--", "--playback"]
        elif command_id == "rom-command-playback-headless":
            command = [
                "cargo",
                "run",
                "--release",
                "--features",
                "sdl",
                "--bin",
                "neser",
                "--",
                "--playback-headless",
            ]

        if command is None:
            self._append_log(f"Unknown ROM command: {command_id}")
            return

        command.append(str(rom_absolute_path))
        self._append_log(f"Running command: {' '.join(command)}")
        playback_result = await self._run_autorun_command_with_status_modal(command, command_id)
        if command_id in {"rom-command-playback-headed", "rom-command-playback-headless"}:
            if playback_result in {"passed", "failed"}:
                self._autorun_playback_results[rom_relative_path] = playback_result
        self._refresh_record_autorun_state(rom_relative_path)

    async def _playback_all_autorun_files(self) -> None:
        """Playback all discovered autorun files sequentially with progress status."""

        autorun_records = sorted(
            [record for record in self.rom_file_records.values() if record.has_autorun],
            key=lambda record: record.rom_path.casefold(),
        )

        if not autorun_records:
            self._append_log("No autorun files found for Playback All")
            return

        total_records = len(autorun_records)
        self._append_log(f"Playback All started for {total_records} autorun file(s)")

        for index, record in enumerate(autorun_records, start=1):
            if self._autorun_cancel_requested:
                break

            rom_absolute_path = self.rom_root / Path(record.rom_path)
            command = [
                "cargo",
                "run",
                "--release",
                "--features",
                "sdl",
                "--bin",
                "neser",
                "--",
                "--playback-headless",
                str(rom_absolute_path),
            ]
            file_progress = f"{index}/{total_records}"
            status_context = f"File {file_progress}: {Path(record.rom_path).name}"
            self._append_log(f"Playback All running ({file_progress}): {record.rom_path}")
            playback_result = await self._run_autorun_command_with_status_modal(
                command,
                "rom-command-playback-headless",
                full_set_progress_status=status_context,
                current_file_progress_status="Checkpoint: -",
            )

            if playback_result in {"passed", "failed"}:
                self._autorun_playback_results[record.rom_path] = playback_result
            self._refresh_record_autorun_state(record.rom_path)

            if self._autorun_cancel_requested:
                break

        if self._autorun_cancel_requested:
            self._append_log("Playback All cancelled")
        else:
            self._append_log("Playback All completed")

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

            async def consume_stream(reader: asyncio.StreamReader | None) -> None:
                nonlocal checkpoints_done, checkpoints_total, error_count
                if reader is None:
                    return

                while True:
                    line_bytes = await reader.readline()
                    if not line_bytes:
                        break

                    line = line_bytes.decode(errors="replace").strip()
                    if not line:
                        continue
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
                                f"Checkpoint {checkpoints_done}/{checkpoints_total}. "
                                f"Errors: {error_count}"
                            ),
                        )

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
        if not has_autorun:
            self._autorun_playback_results.pop(rom_relative_path, None)
        if has_autorun != existing.has_autorun:
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
        try:
            self._append_log("Scan cancel requested")
        except (NoMatches, ScreenStackError):
            pass

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
                try:
                    self._scan_modal.dismiss(None)
                except Exception:
                    pass
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
    def _read_setting_bool(settings: dict[str, Any], key: str) -> bool:
        """Read a boolean setting from the loaded settings payload."""

        value = settings.get(key, False)
        if isinstance(value, bool):
            return value
        if isinstance(value, str):
            return value.strip().lower() in {"1", "true", "yes", "on"}
        if isinstance(value, (int, float)):
            return value != 0
        return False

    def on_data_table_row_highlighted(self, event: DataTable.RowHighlighted) -> None:
        """Show full ROM path tooltip when hovering/highlighting a ROM table row."""

        if event.data_table.id != "rom-database":
            return

        if 0 <= event.cursor_row < len(self._rom_table_full_paths):
            event.data_table.tooltip = self._rom_table_full_paths[event.cursor_row]
            return

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
