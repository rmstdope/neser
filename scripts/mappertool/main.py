"""Entrypoint module for mappertool Textual app."""

import sys
from pathlib import Path

if __package__ in (None, ""):
    repo_root = Path(__file__).resolve().parents[2]
    if str(repo_root) not in sys.path:
        sys.path.insert(0, str(repo_root))
    from scripts.mappertool.mapper_tool_app import MapperToolApp
else:
    from .mapper_tool_app import MapperToolApp


def main() -> None:
    """Run the mappertool Textual app."""

    MapperToolApp().run()


if __name__ == "__main__":
    main()
