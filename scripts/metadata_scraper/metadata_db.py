"""SQLite persistence layer for TheGamesDB metadata."""

import json
import sqlite3
from typing import Any

from scripts.metadata_scraper import JsonDict

_ALLOWED_REFERENCE_TABLES = frozenset({"genres", "developers", "publishers", "regions", "countries"})

_SCHEMA = """
CREATE TABLE IF NOT EXISTS platforms (
    id      INTEGER PRIMARY KEY,
    name    TEXT NOT NULL,
    alias   TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS games (
    id           INTEGER PRIMARY KEY,
    game_title   TEXT NOT NULL,
    release_date TEXT,
    platform_id  INTEGER NOT NULL REFERENCES platforms(id),
    region_id    INTEGER,
    country_id   INTEGER,
    players      INTEGER,
    overview     TEXT,
    last_updated TEXT,
    rating       TEXT,
    coop         TEXT,
    youtube      TEXT,
    alternates   TEXT
);

CREATE TABLE IF NOT EXISTS genres (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS developers (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS publishers (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS regions (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS countries (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS game_genres (
    game_id  INTEGER NOT NULL,
    genre_id INTEGER NOT NULL,
    PRIMARY KEY (game_id, genre_id)
);

CREATE TABLE IF NOT EXISTS game_developers (
    game_id      INTEGER NOT NULL,
    developer_id INTEGER NOT NULL,
    PRIMARY KEY (game_id, developer_id)
);

CREATE TABLE IF NOT EXISTS game_publishers (
    game_id      INTEGER NOT NULL,
    publisher_id INTEGER NOT NULL,
    PRIMARY KEY (game_id, publisher_id)
);

CREATE TABLE IF NOT EXISTS images (
    id         INTEGER PRIMARY KEY,
    game_id    INTEGER NOT NULL REFERENCES games(id),
    type       TEXT,
    side       TEXT,
    filename   TEXT,
    resolution TEXT
);

CREATE TABLE IF NOT EXISTS image_base_urls (
    size TEXT PRIMARY KEY,
    url  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sync_log (
    platform_id            INTEGER PRIMARY KEY REFERENCES platforms(id),
    last_full_sync         TEXT,
    last_incremental_sync  TEXT,
    last_update_id         INTEGER
);
"""


class MetadataDb:
    """SQLite-backed store for TheGamesDB metadata."""

    def __init__(self, db_path: str = "metadata.db") -> None:
        self._conn = sqlite3.connect(db_path)
        self._conn.row_factory = sqlite3.Row
        self._conn.execute("PRAGMA foreign_keys = ON")
        self._apply_schema()

    def _apply_schema(self) -> None:
        self._conn.executescript(_SCHEMA)
        self._conn.commit()

    def close(self) -> None:
        self._conn.close()

    def __enter__(self) -> "MetadataDb":
        return self

    def __exit__(self, *args: object) -> None:
        self.close()

    # ── platforms ────────────────────────────────────────────────────────────

    def upsert_platform(self, platform: JsonDict) -> None:
        self._conn.execute(
            "INSERT INTO platforms (id, name, alias) VALUES (?, ?, ?)"
            " ON CONFLICT(id) DO UPDATE SET name=excluded.name, alias=excluded.alias",
            (platform["id"], platform["name"], platform.get("alias", "")),
        )
        self._conn.commit()

    def get_platform(self, platform_id: int) -> JsonDict | None:
        row = self._conn.execute("SELECT id, name, alias FROM platforms WHERE id = ?", (platform_id,)).fetchone()
        return dict(row) if row else None

    def list_platforms(self) -> list[JsonDict]:
        rows = self._conn.execute("SELECT id, name, alias FROM platforms ORDER BY id").fetchall()
        return [dict(r) for r in rows]

    # ── reference tables (genres, developers, publishers, regions, countries) ─

    def _validate_ref_table(self, table: str) -> None:
        if table not in _ALLOWED_REFERENCE_TABLES:
            raise ValueError(f"Invalid reference table: {table!r}")

    def upsert_reference(self, table: str, entity: JsonDict) -> None:
        self._validate_ref_table(table)
        self._conn.execute(
            f"INSERT INTO {table} (id, name) VALUES (?, ?)"  # noqa: S608 (table is validated)
            f" ON CONFLICT(id) DO UPDATE SET name=excluded.name",
            (entity["id"], entity["name"]),
        )
        self._conn.commit()

    def get_reference(self, table: str, entity_id: int) -> JsonDict | None:
        self._validate_ref_table(table)
        row = self._conn.execute(
            f"SELECT id, name FROM {table} WHERE id = ?",  # noqa: S608 (table is validated above)
            (entity_id,),
        ).fetchone()
        return dict(row) if row else None

    # ── games ────────────────────────────────────────────────────────────────

    def upsert_game(self, game: JsonDict) -> None:
        alternates = game.get("alternates")
        if alternates is not None and not isinstance(alternates, str):
            alternates = json.dumps(alternates)
        self._conn.execute(
            """INSERT INTO games
               (id, game_title, release_date, platform_id, region_id, country_id,
                players, overview, last_updated, rating, coop, youtube, alternates)
               VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)
               ON CONFLICT(id) DO UPDATE SET
                 game_title   = excluded.game_title,
                 release_date = excluded.release_date,
                 platform_id  = excluded.platform_id,
                 region_id    = excluded.region_id,
                 country_id   = excluded.country_id,
                 players      = excluded.players,
                 overview     = excluded.overview,
                 last_updated = excluded.last_updated,
                 rating       = excluded.rating,
                 coop         = excluded.coop,
                 youtube      = excluded.youtube,
                 alternates   = excluded.alternates""",
            (
                game["id"],
                game["game_title"],
                game.get("release_date"),
                game["platform"],
                game.get("region_id"),
                game.get("country_id"),
                game.get("players"),
                game.get("overview"),
                game.get("last_updated"),
                game.get("rating"),
                game.get("coop"),
                game.get("youtube"),
                alternates,
            ),
        )
        self._upsert_join("game_genres", "genre_id", game["id"], game.get("genres") or [])
        self._upsert_join("game_developers", "developer_id", game["id"], game.get("developers") or [])
        self._upsert_join("game_publishers", "publisher_id", game["id"], game.get("publishers") or [])
        self._conn.commit()

    def _upsert_join(self, table: str, fk_col: str, game_id: int, ids: list[int]) -> None:
        # table/fk_col are code constants supplied by upsert_game, never external
        # input, and identifiers cannot be bound as parameters. Values are bound.
        self._conn.execute(f"DELETE FROM {table} WHERE game_id = ?", (game_id,))  # noqa: S608
        for entity_id in ids:
            self._conn.execute(
                f"INSERT OR IGNORE INTO {table} (game_id, {fk_col}) VALUES (?, ?)",  # noqa: S608
                (game_id, entity_id),
            )

    def get_game(self, game_id: int) -> JsonDict | None:
        row = self._conn.execute("SELECT * FROM games WHERE id = ?", (game_id,)).fetchone()
        return dict(row) if row else None

    def list_games(self, platform_id: int | None = None, game_id: int | None = None) -> list[JsonDict]:
        query = "SELECT * FROM games WHERE 1=1"
        params: list[Any] = []
        if platform_id is not None:
            query += " AND platform_id = ?"
            params.append(platform_id)
        if game_id is not None:
            query += " AND id = ?"
            params.append(game_id)
        query += " ORDER BY game_title"
        rows = self._conn.execute(query, params).fetchall()
        return [dict(r) for r in rows]

    def search_games(self, name: str, platform_id: int | None = None) -> list[JsonDict]:
        query = "SELECT * FROM games WHERE LOWER(game_title) LIKE LOWER('%' || ? || '%')"
        params: list[Any] = [name]
        if platform_id is not None:
            query += " AND platform_id = ?"
            params.append(platform_id)
        query += " ORDER BY game_title"
        rows = self._conn.execute(query, params).fetchall()
        return [dict(r) for r in rows]

    def get_game_genres(self, game_id: int) -> list[int]:
        rows = self._conn.execute(
            "SELECT genre_id FROM game_genres WHERE game_id = ? ORDER BY genre_id", (game_id,)
        ).fetchall()
        return [r[0] for r in rows]

    def get_game_developers(self, game_id: int) -> list[int]:
        rows = self._conn.execute(
            "SELECT developer_id FROM game_developers WHERE game_id = ? ORDER BY developer_id",
            (game_id,),
        ).fetchall()
        return [r[0] for r in rows]

    def get_game_publishers(self, game_id: int) -> list[int]:
        rows = self._conn.execute(
            "SELECT publisher_id FROM game_publishers WHERE game_id = ? ORDER BY publisher_id",
            (game_id,),
        ).fetchall()
        return [r[0] for r in rows]

    # ── images ───────────────────────────────────────────────────────────────

    def upsert_image(self, image: JsonDict) -> None:
        self._conn.execute(
            """INSERT INTO images (id, game_id, type, side, filename, resolution)
               VALUES (?,?,?,?,?,?)
               ON CONFLICT(id) DO UPDATE SET
                 game_id    = excluded.game_id,
                 type       = excluded.type,
                 side       = excluded.side,
                 filename   = excluded.filename,
                 resolution = excluded.resolution""",
            (
                image["id"],
                image["game_id"],
                image.get("type"),
                image.get("side"),
                image.get("filename"),
                image.get("resolution"),
            ),
        )
        self._conn.commit()

    def get_game_images(self, game_id: int) -> list[JsonDict]:
        rows = self._conn.execute(
            "SELECT id, game_id, type, side, filename, resolution FROM images WHERE game_id = ?",
            (game_id,),
        ).fetchall()
        return [dict(r) for r in rows]

    def get_game_image_counts_by_type(self, game_id: int) -> dict[str | None, int]:
        """Returns a mapping of image type to count for the given game.

        Types are the values stored in the ``type`` column (e.g. ``"boxart"``,
        ``"screenshot"``, ``"fanart"``).  ``None`` is used for rows where the
        type is not set.
        """
        rows = self._conn.execute(
            "SELECT type, COUNT(*) AS cnt FROM images WHERE game_id = ? GROUP BY type",
            (game_id,),
        ).fetchall()
        return {r["type"]: r["cnt"] for r in rows}

    def upsert_image_base_urls(self, base_urls: dict[str, str]) -> None:
        for size, url in base_urls.items():
            self._conn.execute(
                "INSERT INTO image_base_urls (size, url) VALUES (?, ?)"
                " ON CONFLICT(size) DO UPDATE SET url=excluded.url",
                (size, url),
            )
        self._conn.commit()

    def get_image_base_urls(self) -> dict[str, str]:
        rows = self._conn.execute("SELECT size, url FROM image_base_urls").fetchall()
        return {r["size"]: r["url"] for r in rows}

    def build_image_url(self, image_id: int, size: str) -> str:
        base_urls = self.get_image_base_urls()
        base = base_urls[size]  # raises KeyError if size unknown
        row = self._conn.execute("SELECT filename FROM images WHERE id = ?", (image_id,)).fetchone()
        if row is None:
            raise KeyError(f"No image with id={image_id}")
        filename: str = row["filename"]
        return base + filename

    # ── sync log ─────────────────────────────────────────────────────────────

    def get_sync_log(self, platform_id: int) -> JsonDict | None:
        row = self._conn.execute(
            "SELECT platform_id, last_full_sync, last_incremental_sync, last_update_id"
            " FROM sync_log WHERE platform_id = ?",
            (platform_id,),
        ).fetchone()
        return dict(row) if row else None

    def update_sync_log(
        self,
        platform_id: int,
        last_update_id: int | None = None,
        last_full_sync: str | None = None,
        last_incremental_sync: str | None = None,
    ) -> None:
        existing = self.get_sync_log(platform_id)
        if existing is None:
            self._conn.execute(
                "INSERT INTO sync_log (platform_id, last_full_sync, last_incremental_sync, last_update_id)"
                " VALUES (?,?,?,?)",
                (platform_id, last_full_sync, last_incremental_sync, last_update_id),
            )
        else:
            updates: JsonDict = {}
            if last_full_sync is not None:
                updates["last_full_sync"] = last_full_sync
            if last_incremental_sync is not None:
                updates["last_incremental_sync"] = last_incremental_sync
            if last_update_id is not None:
                updates["last_update_id"] = last_update_id
            if updates:
                set_clause = ", ".join(f"{k}=?" for k in updates)
                params = [*list(updates.values()), platform_id]
                self._conn.execute(
                    # set_clause is built from literal column names above; values are bound.
                    f"UPDATE sync_log SET {set_clause} WHERE platform_id=?",  # noqa: S608
                    params,
                )
        self._conn.commit()

    # ── statistics ───────────────────────────────────────────────────────────

    def get_game_counts(self) -> dict[int, int]:
        rows = self._conn.execute("SELECT platform_id, COUNT(*) AS cnt FROM games GROUP BY platform_id").fetchall()
        return {r["platform_id"]: r["cnt"] for r in rows}
