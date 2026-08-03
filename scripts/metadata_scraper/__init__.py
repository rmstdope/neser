"""TheGamesDB metadata scraper package."""

from typing import Any

# TheGamesDB returns free-form JSON objects, and the SQLite rows mirror that
# shape, so both layers pass dictionaries around rather than modelling every
# endpoint's schema.
JsonDict = dict[str, Any]
