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
from romxml import RomXml
from nescartdb import NesCartDb, BASE_URL
from rom_database import RomDatabase

def print_csv_header(field_order: list[str]) -> None:
    """Print a CSV header describing the exported fields."""
    print("# NES ROM information (CSV format)")
    print("# Fields: " + ", ".join(field_order))
    print("# Each line is a single ROM entry. Empty fields are left blank.")

def parse_arguments():
    parser = argparse.ArgumentParser(description="Scrape NES Cart Database ROM data")
    parser.add_argument(
        "--csv",
        action="store_true",
        help="Output as CSV with comments instead of JSON."
    )
    parser.add_argument(
        "--db",
        default="roms.sqlite",
        help="SQLite database file to store scraped ROM data."
    )

    subparsers = parser.add_subparsers(dest="command", required=True)
    list_parser = subparsers.add_parser("list", help="List all ROM entries in the database")
    list_parser.set_defaults(command="list")

    scrape_parser = subparsers.add_parser("scrape", help="Scrape a range of ROM ids")
    scrape_parser.add_argument(
        "rom_id",
        help="ROM profile ID or range (xxxx-yyyy) from nescartdb.com"
    )
    scrape_parser.add_argument(
        "--url",
        default=BASE_URL,
        help="Override profile URL (default: https://nescartdb.com/profile/view/<id>)",
    )
    scrape_parser.set_defaults(command="scrape")

    import_parser = subparsers.add_parser("import", help="Import XML file and merge entries into the database")
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
            if args.csv:
                print_csv_header(field_order)
                for row in rows:
                    cleaned = {
                        k: v
                        for k, v in row.items()
                        if v is not None and not (k.endswith("size") and v == 0)
                    }
                    print(",".join(str(cleaned.get(k, "") or "") for k in field_order))
            else:
                cleaned_rows = [
                    {
                        k: v
                        for k, v in row.items()
                        if v is not None
                    }
                    for row in rows
                ]
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
