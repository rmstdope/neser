"""Main Textual application for mappertool."""

from pathlib import Path

from textual.app import App, ComposeResult
from textual.containers import Horizontal, Vertical
from textual.widgets import DataTable, Input, ProgressBar, TextArea

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
    DEFAULT_ROM_ROOT = Path("roms/games")
    DEFAULT_ROM_FILES_DB_PATH = Path("scripts/mappertool/rom_files.csv")
    CSS = """
    Screen {
        layout: vertical;
    }

    #autorun-progress {
        height: 3;
        width: 1fr;
        border: solid $accent;
        padding: 0 1;
    }

    #main-panes {
        layout: horizontal;
        height: 1fr;
    }

    .pane {
        width: 1fr;
        border: solid $panel;
        padding: 0 1;
    }

    #rom-pane {
        layout: vertical;
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
    """

    def __init__(
        self,
        rom_db_csv_path: Path | None = None,
        rom_root: Path | None = None,
        rom_files_csv_path: Path | None = None,
    ) -> None:
        super().__init__()
        self.rom_db_csv_path = self._resolve_repo_path(rom_db_csv_path or self.DEFAULT_ROM_DB_PATH)
        self.rom_root = self._resolve_repo_path(rom_root or self.DEFAULT_ROM_ROOT)
        self.rom_files_csv_path = self._resolve_repo_path(
            rom_files_csv_path or self.DEFAULT_ROM_FILES_DB_PATH
        )
        self.rom_db_index = RomDbIndex({})
        self.rom_file_records: dict[str, RomFileRecord] = {}
        self._rom_table_full_paths: list[str] = []
        self._rom_sort_column_index: int | None = None
        self._rom_sort_reverse = False
        self._rom_name_filter = ""
        self._rom_mapper_filter_text = ""
        self._rom_mapper_filter_values: set[int] = set()

    def compose(self) -> ComposeResult:
        """Build the top progress row and the three lower panes."""

        yield ProgressBar(total=100, id="autorun-progress")
        with Horizontal(id="main-panes"):
            with Vertical(id="rom-pane", classes="pane"):
                with Horizontal(id="rom-filters"):
                    yield Input(
                        placeholder="Mapper filter (comma separated)",
                        id="mapper-filter-input",
                    )
                    yield Input(
                        placeholder="ROM name filter",
                        id="name-filter-input",
                    )
                rom_database = DataTable(id="rom-database")
                rom_database.add_columns("Map", "SMap", "ROM", "CRC", "Source")
                yield rom_database

            logs = TextArea(id="logs", classes="pane", read_only=True)
            logs.load_text("Mappertool logs will appear here.\n")
            yield logs

            config_editor = TextArea(id="config-editor", classes="pane")
            config_editor.load_text(
                "# Mappertool config\n"
                "neser_binary = ./target/release/neser\n"
                "rom_root = ./roms/games\n"
                "autorun_root = ./roms/automated_tests\n"
            )
            yield config_editor

    def on_mount(self) -> None:
        """Load databases and scan for new ROM files at startup."""

        rom_table = self.query_one("#rom-database", DataTable)

        try:
            self.rom_db_index = RomDbIndex.from_csv(self.rom_db_csv_path)
            self._append_log(
                f"Loaded ROM database: {self.rom_db_index.size} entries from {self.rom_db_csv_path}"
            )
        except FileNotFoundError:
            self._append_log(f"ROM database not found: {self.rom_db_csv_path}")

        rom_file_db = RomFileDatabase(self.rom_files_csv_path)
        (
            self.rom_file_records,
            new_records,
            updated_records,
            invalid_marked,
            warnings,
        ) = rom_file_db.scan_and_update(
            self.rom_root,
            self.rom_db_index,
        )

        for warning in warnings:
            self._append_log(warning)

        self._populate_rom_table(rom_table)
        self._append_log(
            "ROM inventory: "
            f"{len(self.rom_file_records)} known, "
            f"{len(new_records)} new, "
            f"{updated_records} updated, "
            f"{invalid_marked} invalid-marked from {self.rom_root}"
        )

    def lookup_rom_by_crc(self, crc: int | str) -> RomDbEntry | None:
        """Return a ROM DB entry matching a full-ROM CRC value."""

        return self.rom_db_index.lookup(crc)

    def _populate_rom_table(self, rom_table: DataTable) -> None:
        """Render tracked ROM records in the left-hand ROM list widget."""

        rom_table.clear(columns=False)
        self._rom_table_full_paths = []
        for record in self._sorted_rom_records():
            rom_table.add_row(
                record.mapper,
                record.submapper,
                self._rom_display_name(record.rom_path),
                record.crc,
                record.mapper_source,
            )
            self._rom_table_full_paths.append(record.rom_path)
        self._update_rom_table_summary(rom_table)

    @staticmethod
    def _rom_display_name(rom_path: str) -> str:
        """Return filename-only display text for a ROM path."""

        return Path(rom_path).name

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
            rom_name = self._rom_display_name(record.rom_path).casefold()
            if name_filter not in rom_name:
                return False

        if self._rom_mapper_filter_values:
            mapper_value = self._parse_mapper_number(record.mapper)
            if mapper_value is None:
                return False

            if mapper_value not in self._rom_mapper_filter_values:
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
            return (0, self._rom_display_name(record.rom_path).casefold())
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
            self._rom_mapper_filter_text = event.value
            self._rom_mapper_filter_values = self.parse_mapper_filter_values(event.value)
        elif event.input.id == "name-filter-input":
            self._rom_name_filter = event.value
        else:
            return

        rom_table = self.query_one("#rom-database", DataTable)
        self._populate_rom_table(rom_table)

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
