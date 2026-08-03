"""CLI entrypoint for the TheGamesDB metadata scraper."""

import argparse
import json
import os
import sys
from typing import IO

try:
    import requests_cache

    _HAVE_REQUESTS_CACHE = True
except ImportError:
    _HAVE_REQUESTS_CACHE = False

from scripts.metadata_scraper.api_client import ApiError, TheGamesDbClient
from scripts.metadata_scraper.metadata_db import MetadataDb
from scripts.metadata_scraper.sync import Syncer

# ── platform registry ─────────────────────────────────────────────────────────

PLATFORMS = {
    "nes": {"id": 7, "name": "Nintendo Entertainment System (NES)", "alias": "nintendo-entertainment-system-nes"},
    "snes": {"id": 6, "name": "Super Nintendo (SNES)", "alias": "super-nintendo-snes"},
    "gb": {"id": 4, "name": "Nintendo Game Boy", "alias": "nintendo-gameboy"},
    "gbc": {"id": 41, "name": "Nintendo Game Boy Color", "alias": "nintendo-gameboy-color"},
    "gba": {"id": 5, "name": "Nintendo Game Boy Advance", "alias": "nintendo-gameboy-advance"},
}

DEFAULT_DB = os.path.join(os.path.dirname(__file__), "metadata.db")


# ── argument parser ───────────────────────────────────────────────────────────


def _build_parser() -> argparse.ArgumentParser:
    # Common args shared by all subcommands (so --api-key/--db can appear after the verb)
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--api-key", metavar="KEY", help="TheGamesDB API key (overrides THEGAMESDB_API_KEY env var)")
    common.add_argument(
        "--db", metavar="PATH", default=DEFAULT_DB, help=f"Path to SQLite database (default: {DEFAULT_DB})"
    )
    common.add_argument(
        "--cache", action="store_true", help="Cache HTTP responses (dev mode, avoids re-spending API quota)"
    )
    common.add_argument(
        "--cache-path",
        metavar="PATH",
        default="thegamesdb_cache",
        help="SQLite cache file path without extension (default: thegamesdb_cache)",
    )

    parser = argparse.ArgumentParser(
        prog="metadata_scraper",
        description="Fetch and cache game metadata from TheGamesDB.",
        parents=[common],
    )

    sub = parser.add_subparsers(dest="command", required=True)

    # sync
    p_sync = sub.add_parser("sync", help="Sync game metadata from TheGamesDB", parents=[common])
    p_sync.add_argument(
        "--platform", default="nes", choices=[*list(PLATFORMS), "all"], help="Platform(s) to sync (default: nes)"
    )
    p_sync.add_argument(
        "--force-full", action="store_true", help="Force a full re-sync even if incremental data exists"
    )

    # list
    p_list = sub.add_parser("list", help="List stored games", parents=[common])
    p_list.add_argument("--platform", choices=list(PLATFORMS), help="Filter by platform")
    p_list.add_argument("--game-id", type=int, metavar="ID", help="Filter by game id")
    p_list.add_argument("--format", dest="fmt", choices=["table", "json"], default="table")

    # images
    p_img = sub.add_parser("images", help="Show image URLs for a game", parents=[common])
    p_img.add_argument("game_id", type=int, metavar="GAME_ID")
    p_img.add_argument(
        "--size", choices=["original", "small", "thumb", "cropped_center_thumb", "medium", "large"], default="original"
    )

    # status
    sub.add_parser("status", help="Show DB statistics and remaining API allowance", parents=[common])

    # info
    p_info = sub.add_parser("info", help="Search for games by name and display details", parents=[common])
    p_info.add_argument("name", metavar="NAME", help="Substring to search for (case-insensitive)")
    p_info.add_argument("--platform", choices=list(PLATFORMS), help="Limit search to one platform")

    return parser


# ── command handlers ──────────────────────────────────────────────────────────


def _resolve_api_key(args: argparse.Namespace) -> str:
    key = getattr(args, "api_key", None) or os.environ.get("THEGAMESDB_API_KEY", "")
    if not key:
        print("ERROR: API key required. Set THEGAMESDB_API_KEY or use --api-key.", file=sys.stderr)
        sys.exit(1)
    return key


def _cmd_sync(args: argparse.Namespace, db: MetadataDb, output: IO[str]):
    key = _resolve_api_key(args)
    client = TheGamesDbClient(api_key=key, verbose=True)
    syncer = Syncer(db=db, client=client, verbose=True)

    platforms_to_sync = (
        list(PLATFORMS.items()) if args.platform == "all" else [(args.platform, PLATFORMS[args.platform])]
    )

    for slug, info in platforms_to_sync:
        platform_info = {**info, "slug": slug}
        print(f"Syncing {info['name']} …", file=output)
        syncer.sync(platform_id=info["id"], platform_info=platform_info, force_full=args.force_full)
        print("  Done.", file=output)


def _cmd_list(args: argparse.Namespace, db: MetadataDb, output: IO[str]):
    kwargs = {}
    if getattr(args, "platform", None):
        kwargs["platform_id"] = PLATFORMS[args.platform]["id"]
    if getattr(args, "game_id", None) is not None:
        kwargs["game_id"] = args.game_id

    games = db.list_games(**kwargs)

    if args.fmt == "json":
        print(json.dumps(games, indent=2), file=output)
    else:
        if not games:
            print("No games found.", file=output)
            return
        print(f"{'ID':>6}  {'Platform':>4}  Title", file=output)
        print("-" * 60, file=output)
        for g in games:
            print(f"{g['id']:>6}  {g['platform_id']:>4}  {g['game_title']}", file=output)


def _cmd_images(args: argparse.Namespace, db: MetadataDb, output: IO[str]):
    images = db.get_game_images(args.game_id)
    if not images:
        print(f"No images found for game id {args.game_id}.", file=output)
        return
    for img in images:
        try:
            url = db.build_image_url(img["id"], args.size)
        except KeyError:
            url = img["filename"]
        side = f" ({img['side']})" if img.get("side") else ""
        print(f"[{img['type']}{side}] {url}", file=output)


def _cmd_status(args: argparse.Namespace, db: MetadataDb, client: TheGamesDbClient, output: IO[str]):
    counts = db.get_game_counts()
    platforms = {p["id"]: p for p in db.list_platforms()}

    print("=== Local DB ===", file=output)
    if not counts:
        print("  No games stored yet.", file=output)
    else:
        for pid, cnt in sorted(counts.items()):
            name = platforms.get(pid, {}).get("name", f"Platform {pid}")
            print(f"  {name}: {cnt} games", file=output)

    print("", file=output)
    try:
        limit = client.get_api_limit()
        remaining = limit.get("remaining_monthly_allowance", "?")
        extra = limit.get("extra_allowance", 0)
        print("=== API Allowance ===", file=output)
        print(f"  Remaining: {remaining}  Extra: {extra}", file=output)
    except Exception as exc:
        print(f"  Could not fetch API limit: {exc}", file=output)


def _cmd_info(args: argparse.Namespace, db: MetadataDb, output: IO[str]):
    platform_id = PLATFORMS[args.platform]["id"] if getattr(args, "platform", None) else None
    games = db.search_games(args.name, platform_id=platform_id)

    if not games:
        print(f"No games found matching '{args.name}'.", file=output)
        return

    platforms = {p["id"]: p for p in db.list_platforms()}

    for game in games:
        pid = game.get("platform_id")
        platform_name = platforms.get(pid, {}).get("alias") or platforms.get(pid, {}).get("name") or str(pid)
        print(f"Game #{game['id']} — {game['game_title']} ({platform_name.upper()})", file=output)

        def _resolve_names(ids, table):
            names = []
            for eid in ids:
                row = db.get_reference(table, eid)
                if row:
                    names.append(row["name"])
            return ", ".join(names) if names else ""

        genre_ids = db.get_game_genres(game["id"])
        dev_ids = db.get_game_developers(game["id"])
        pub_ids = db.get_game_publishers(game["id"])
        image_counts = db.get_game_image_counts_by_type(game["id"])
        total_images = sum(image_counts.values())
        if image_counts:
            type_summary = ", ".join(
                f"{t or 'unknown'}: {c}" for t, c in sorted(image_counts.items(), key=lambda x: x[0] or "")
            )
            images_value = f"{total_images} ({type_summary})"
        else:
            images_value = "0"

        fields = [
            ("Release date", game.get("release_date") or ""),
            ("Rating", game.get("rating") or ""),
            ("Players", game.get("players") or ""),
            ("Co-op", game.get("coop") or ""),
            ("Overview", game.get("overview") or ""),
            ("Genres", _resolve_names(genre_ids, "genres")),
            ("Developers", _resolve_names(dev_ids, "developers")),
            ("Publishers", _resolve_names(pub_ids, "publishers")),
            ("YouTube", game.get("youtube") or ""),
            ("Alternates", game.get("alternates") or ""),
            ("Last updated", game.get("last_updated") or ""),
            ("Images", images_value),
        ]
        width = max(len(label) for label, _ in fields)
        for label, value in fields:
            print(f"  {label:<{width}} : {value}", file=output)
        print("", file=output)


# ── main ──────────────────────────────────────────────────────────────────────


def main(output: IO[str] | None = None):
    if output is None:
        output = sys.stdout

    parser = _build_parser()
    args = parser.parse_args()

    if getattr(args, "cache", False):
        if not _HAVE_REQUESTS_CACHE:
            print("ERROR: requests-cache is not installed. Run: pip install requests-cache", file=sys.stderr)
            sys.exit(1)
        cache_path = getattr(args, "cache_path", "thegamesdb_cache")
        requests_cache.install_cache(
            cache_path,
            backend="sqlite",
            expire_after=None,
        )
        print(f"[cache] HTTP responses cached to {cache_path}.sqlite", file=sys.stderr)

    try:
        with MetadataDb(args.db) as db:
            if args.command == "sync":
                _cmd_sync(args, db, output)
            elif args.command == "list":
                _cmd_list(args, db, output)
            elif args.command == "images":
                _cmd_images(args, db, output)
            elif args.command == "status":
                key = _resolve_api_key(args)
                client = TheGamesDbClient(api_key=key)
                _cmd_status(args, db, client, output)
            elif args.command == "info":
                _cmd_info(args, db, output)
    except ApiError as exc:
        if exc.status_code == 429:
            print("ERROR: TheGamesDB request limit reached (HTTP 429).", file=sys.stderr)
            print("Your monthly API allowance appears to be used up. Check what is left with", file=sys.stderr)
            print("the 'status' command, wait for the allowance to reset, or use --cache during", file=sys.stderr)
            print("development to avoid re-spending quota on repeated requests.", file=sys.stderr)
        else:
            print(f"ERROR: TheGamesDB request failed: {exc}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
