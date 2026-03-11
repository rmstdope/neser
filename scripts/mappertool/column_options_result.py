"""Result model for ROM filter dialog."""

from dataclasses import dataclass


@dataclass(frozen=True)
class ColumnOptionsResult:
    """Result payload from the ROM filter dialog."""

    sort_option: str
    name_filter: str
    mapper_filter_text: str
    mapper_filter_ranges: list[tuple[int, int]]
