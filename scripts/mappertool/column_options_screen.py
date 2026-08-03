"""Modal dialog for ROM table sorting and filtering."""

from typing import TYPE_CHECKING, ClassVar

from textual.app import ComposeResult
from textual.containers import Horizontal, Vertical
from textual.screen import ModalScreen
from textual.widgets import Button, Input, Label

if TYPE_CHECKING:
    pass


class FilterDialogScreen(ModalScreen[None]):
    """Dialog for global ROM sorting and filtering."""

    BINDINGS: ClassVar[list[tuple[str, str, str]]] = [
        ("enter", "apply", "Apply"),
        ("escape", "cancel", "Cancel"),
    ]

    SORT_OPTIONS: ClassVar[dict[str, str]] = {
        "sort-mapper-asc": "mapper_asc",
        "sort-mapper-desc": "mapper_desc",
        "sort-name-asc": "name_asc",
        "sort-name-desc": "name_desc",
    }

    def __init__(
        self,
        current_sort_option: str,
        current_name_filter: str,
        current_mapper_filter: str,
    ) -> None:
        super().__init__()
        self.current_sort_option = current_sort_option
        self.current_name_filter = current_name_filter
        self.current_mapper_filter = current_mapper_filter

    def compose(self) -> ComposeResult:
        """Render controls for sorting, name filter, and mapper filter."""

        with Vertical(id="filter-dialog"):
            yield Label("ROM Filter")
            yield Label("Sorting")
            with Horizontal(id="filter-dialog-sort-buttons"):
                yield Button("Mapper asc.", id="sort-mapper-asc")
                yield Button("Mapper desc.", id="sort-mapper-desc")
                yield Button("Name asc.", id="sort-name-asc")
                yield Button("Name desc.", id="sort-name-desc")

            yield Label("Filter by name")
            yield Input(
                value=self.current_name_filter,
                placeholder="ROM filename contains...",
                id="name-filter",
            )

            yield Label("Filter by mapper (e.g. 1,4,24-27)")
            yield Input(
                value=self.current_mapper_filter,
                placeholder="comma-separated values and/or ranges",
                id="mapper-filter",
            )
            yield Label("", id="mapper-filter-error")

            with Horizontal(id="filter-dialog-actions"):
                yield Button("Apply", id="apply-filter", variant="primary")
                yield Button("Cancel", id="cancel-filter")

    def on_mount(self) -> None:
        """Focus name filter field and reflect active sort button."""

        self.query_one("#name-filter", Input).focus()
        if self.current_sort_option == "mapper_asc":
            self.query_one("#sort-mapper-asc", Button).variant = "primary"
        elif self.current_sort_option == "mapper_desc":
            self.query_one("#sort-mapper-desc", Button).variant = "primary"
        elif self.current_sort_option == "name_asc":
            self.query_one("#sort-name-asc", Button).variant = "primary"
        elif self.current_sort_option == "name_desc":
            self.query_one("#sort-name-desc", Button).variant = "primary"

    def on_button_pressed(self, event: Button.Pressed) -> None:
        """Handle sort/apply/cancel button actions."""

        button_id = event.button.id
        if button_id == "cancel-filter":
            self.dismiss(None)
            return

        if button_id in self.SORT_OPTIONS:
            self._apply_and_close(self.SORT_OPTIONS[button_id])
            return

        if button_id == "apply-filter":
            self._apply_and_close(self.current_sort_option)

    def action_apply(self) -> None:
        """Apply current fields with current sort option."""

        self._apply_and_close(self.current_sort_option)

    def action_cancel(self) -> None:
        """Cancel dialog without applying."""

        self.dismiss(None)

    def _apply_and_close(self, sort_option: str) -> None:
        """Validate mapper filter and apply settings if valid."""

        name_filter_input = self.query_one("#name-filter", Input)
        mapper_filter_input = self.query_one("#mapper-filter", Input)
        mapper_error = self.query_one("#mapper-filter-error", Label)

        mapper_text = mapper_filter_input.value.strip()
        mapper_ranges, parse_error = self.parse_mapper_filter_expression(mapper_text)
        if parse_error is not None:
            mapper_error.update(f"Invalid mapper filter: {parse_error}")
            return

        mapper_error.update("")

        app = self.app
        if hasattr(app, "apply_filter_options"):
            typed_app = app
            typed_app.apply_filter_options(
                sort_option=sort_option,
                name_filter=name_filter_input.value,
                mapper_filter_text=mapper_text,
                mapper_filter_ranges=mapper_ranges,
            )

        self.dismiss(None)

    @staticmethod
    def parse_mapper_filter_expression(raw_value: str) -> tuple[list[tuple[int, int]], str | None]:
        """Parse mapper filter values/ranges from comma-separated expression."""

        stripped = raw_value.strip()
        if not stripped:
            return [], None

        ranges: list[tuple[int, int]] = []
        for raw_part in stripped.split(","):
            part = raw_part.strip()
            if not part:
                return [], "empty segment"

            if "-" in part:
                pieces = [piece.strip() for piece in part.split("-", maxsplit=1)]
                if len(pieces) != 2 or not pieces[0] or not pieces[1]:
                    return [], f"invalid range '{part}'"

                try:
                    lower = int(pieces[0], 10)
                    upper = int(pieces[1], 10)
                except ValueError:
                    return [], f"invalid range '{part}'"

                if lower > upper:
                    return [], f"range start greater than end in '{part}'"

                ranges.append((lower, upper))
                continue

            try:
                value = int(part, 10)
            except ValueError:
                return [], f"invalid value '{part}'"

            ranges.append((value, value))

        return ranges, None
