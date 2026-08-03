#!/usr/bin/env python3
"""Scrape ROM information from nescartdb.com.
This module provides utilities to fetch and parse NES cartridge information
from the NES Cart Database (nescartdb.com). It extracts detailed ROM metadata
including console type, video system, mapper information, memory sizes, and
hardware specifications.
The main entry point accepts a ROM profile ID and outputs the parsed data
as JSON containing fields like PRG/CHR ROM sizes, video system (NTSC/PAL),
mapper details, and various hardware configurations.
Requires BeautifulSoup4 for HTML parsing.
"""
import argparse
import json
from scripts.nes_rom_db_scraper.nescartdb import NesCartDb, BASE_URL
from scripts.nes_rom_db_scraper.rom_database import RomDatabase
from scripts.nes_rom_db_scraper.romxml import RomXml


def _filter_present_fields(row: dict) -> dict:
    """Return only fields with non-NULL values, preserving explicit zero values."""
    return {k: v for k, v in row.items() if v is not None}


def _csv_cell(value: object) -> str:
    """Format a CSV cell value, preserving numeric zero and blanking only NULL."""
    return "" if value is None else str(value)

def print_csv_header(field_order: list[str]) -> None:
    """Print a CSV header describing the exported fields."""
    print("# NES ROM information (CSV format)")
    print("# Fields: " + ", ".join(field_order))
    print("# Each line is a single ROM entry. Empty fields are left blank.")

def parse_arguments():
    parser = argparse.ArgumentParser(
        description="Scrape NES Cart Database ROM data",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=(
            "Examples:\n"
            "  # python main.py list json\n"
            "  # python main.py list csv\n"
            "  # python main.py scrape 123\n"
            "  # python main.py scrape 100-200\n"
            "  # python main.py import nes20db.xml"
        ),
    )
    parser.add_argument(
        "--db",
        default="roms.sqlite",
        help="SQLite database file to store scraped ROM data."
    )

    subparsers = parser.add_subparsers(dest="command", required=True)
    list_parser = subparsers.add_parser(
        "list",
        help="List all ROM entries in the database",
        description="List ROM entries with a required output format (json or csv).",
    )
    list_parser.add_argument(
        "format",
        choices=["json", "csv"],
        help="Output format for list results",
    )
    list_parser.set_defaults(command="list")

    scrape_parser = subparsers.add_parser(
        "scrape",
        help="Scrape a range of ROM ids from nescartdb.com and merge into the database",
        description=(
            "Scrape ROM profiles by id, range, or comma-separated list "
            "(e.g. 123, 100-200, or 123,200-250). The keyword 'all' "
            "can be used to scrape every profile."
        ),
    )
    scrape_parser.add_argument(
        "rom_id",
        help=(
            "ROM profile id, range (xxxx-yyyy), or comma-separated list "
            "of ids/ranges from nescartdb.com; use 'all' to scrape every profile"
        ),
    )
    scrape_parser.add_argument(
        "--url",
        default=BASE_URL,
        help="Override profile URL (default: https://nescartdb.com/profile/view/<id>)",
    )
    scrape_parser.set_defaults(command="scrape")

    import_parser = subparsers.add_parser(
        "import",
        help="Import XML file and merge into the database",
        description="Import ROM entries from a NES 2.0 XML file.",
    )
    import_parser.add_argument("filename", help="XML file to import")
    import_parser.set_defaults(command="import")

    drop_parser = subparsers.add_parser("drop", help="Drop and recreate the database schema (destructive)")
    drop_parser.set_defaults(command="drop")

    args = parser.parse_args()
    return args

def main() -> int:
    """
    Main entry point for scraping NES Cart Database ROM information.

    Parses command-line arguments to get a ROM profile ID from nescartdb.com,
    fetches the HTML content from the profile page, parses it using BeautifulSoup,
    and outputs the extracted ROM information as formatted JSON.

    Returns:
        int: Exit code (0 for success, 1 if beautifulsoup4 dependency is missing)

    Raises:
        SystemExit: If required arguments are not provided or argument parsing fails
    """
    args = parse_arguments()

    db = RomDatabase(args.db)
    try:
        # Output order for CSV and listing
        field_order = db.list_columns()
        if args.command == "list":
            rows = db.list_roms()
            if args.format == "csv":
                print_csv_header(field_order)
                for row in rows:
                    cleaned = _filter_present_fields(row)
                    print(",".join(_csv_cell(cleaned.get(k)) for k in field_order))
            else:
                cleaned_rows = [_filter_present_fields(row) for row in rows]
                print(json.dumps(cleaned_rows, indent=2))
            return 0

        if args.command == "drop":
            # Destructive operation: drop and recreate schema
            db.reset_schema()
            print("Database schema reset (dropped and recreated).")
            return 0

        importer = None
        if args.command == "scrape":
            importer = NesCartDb(args.rom_id, base_url=args.url)
        elif args.command == "import":
            importer = RomXml(args.filename)

        if importer is not None:
            added_count = 0
            updated_count = 0
            skipped_count = 0
            conflict_count = 0

            total = importer.num_left()
            while True:
                data = importer.next_record()
                if data is None:
                    break

                # progress bar
                bar_width = 30
                processed = total - importer.num_left()
                filled = int((processed / total) * bar_width)
                progbar = "#" * filled + " " * (bar_width - filled)
                line = f"progress: [{progbar}] {processed}/{total}"
                print(line, end="\r")

                # Delegate processing of a single record to RomDatabase
                a, u, s, c = db.process_record_by_crc(data)
                added_count += a
                updated_count += u
                skipped_count += s
                conflict_count += c
            print(
                "import: added="
                + str(added_count)
                + ", updated="
                + str(updated_count)
                + ", skipped="
                + str(skipped_count)
                + ", conflicts="
                + str(conflict_count)
            )
            return 0

        # total = len(ids)
        # bar_width = 30

        # scraper = NesCartDb(ids, base_url=args.url) if args.url else NesCartDb(ids)

        # added_count = 0
        # updated_count = 0
        # skipped_count = 0
        # conflict_count = 0

        # processed = 0
        # while True:
        #     data = scraper.next_record()
        #     if data is None:
        #         break
        #     processed += 1
        #     # progress bar
        #     if total > 0:
        #         filled = int((processed / total) * bar_width)
        #     else:
        #         filled = 0
        #     progbar = "#" * filled + " " * (bar_width - filled)
        #     line = f"progress: [{progbar}] {processed}/{total}"
        #     print(line, end="\r")

        #     # Ensure minimal required fields
        #     if not data.get("name") or not data.get("crc"):
        #         continue

        #     # Optionally print CSV line
        #     if args.csv:
        #         row = {k: (data.get(k, "") or "") for k in field_order}
        #         print(
        #             ",".join(str(row.get(k, "") or "") for k in field_order)
        #         )

        #     a, u, s, c = db.process_record_by_crc(data)
        #     added_count += a
        #     updated_count += u
        #     skipped_count += s
        #     conflict_count += c

        # summary = (
        #     "scrape: added="
        #     + str(added_count)
        #     + ", updated="
        #     + str(updated_count)
        #     + ", skipped="
        #     + str(skipped_count)
        #     + ", conflicts="
        #     + str(conflict_count)
        # )
        # clear_width = max(len(summary), len("progress: [" + " " * bar_width + "] " + str(total) + "/" + str(total)))
        # print(" " * clear_width, end="\r")
        # print(summary)
        # return 0
    finally:
        db.close()

if __name__ == "__main__":
    raise SystemExit(main())
