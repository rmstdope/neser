"""Tests for MetadataDb — runs entirely in-memory, no network."""

import unittest

from scripts.metadata_scraper.metadata_db import MetadataDb


class TestMetadataDbSchema(unittest.TestCase):
    def setUp(self):
        self.db = MetadataDb(":memory:")

    def tearDown(self):
        self.db.close()

    def _table_names(self):
        cur = self.db._conn.execute("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        return {row[0] for row in cur.fetchall()}

    def test_all_expected_tables_are_created(self):
        tables = self._table_names()
        for expected in (
            "platforms",
            "games",
            "genres",
            "developers",
            "publishers",
            "regions",
            "countries",
            "game_genres",
            "game_developers",
            "game_publishers",
            "images",
            "image_base_urls",
            "sync_log",
        ):
            self.assertIn(expected, tables)


class TestMetadataDbPlatforms(unittest.TestCase):
    def setUp(self):
        self.db = MetadataDb(":memory:")

    def tearDown(self):
        self.db.close()

    def test_upsert_platform_inserts_new_row(self):
        self.db.upsert_platform({"id": 7, "name": "Nintendo Entertainment System (NES)", "alias": "nes"})
        row = self.db.get_platform(7)
        self.assertIsNotNone(row)
        self.assertEqual(row["name"], "Nintendo Entertainment System (NES)")
        self.assertEqual(row["alias"], "nes")

    def test_upsert_platform_updates_existing_row(self):
        self.db.upsert_platform({"id": 7, "name": "Old Name", "alias": "old"})
        self.db.upsert_platform({"id": 7, "name": "New Name", "alias": "new"})
        row = self.db.get_platform(7)
        self.assertEqual(row["name"], "New Name")

    def test_get_platform_returns_none_for_unknown_id(self):
        self.assertIsNone(self.db.get_platform(9999))

    def test_list_platforms_returns_all_platforms(self):
        self.db.upsert_platform({"id": 7, "name": "NES", "alias": "nes"})
        self.db.upsert_platform({"id": 4, "name": "Game Boy", "alias": "gb"})
        platforms = self.db.list_platforms()
        self.assertEqual(len(platforms), 2)


class TestMetadataDbReferenceData(unittest.TestCase):
    def setUp(self):
        self.db = MetadataDb(":memory:")

    def tearDown(self):
        self.db.close()

    def test_upsert_genre_and_retrieve(self):
        self.db.upsert_reference("genres", {"id": 15, "name": "Action"})
        row = self.db.get_reference("genres", 15)
        self.assertIsNotNone(row)
        self.assertEqual(row["name"], "Action")

    def test_upsert_developer_and_retrieve(self):
        self.db.upsert_reference("developers", {"id": 389, "name": "Nintendo"})
        row = self.db.get_reference("developers", 389)
        self.assertEqual(row["name"], "Nintendo")

    def test_upsert_publisher_and_retrieve(self):
        self.db.upsert_reference("publishers", {"id": 252, "name": "Nintendo of America"})
        row = self.db.get_reference("publishers", 252)
        self.assertEqual(row["name"], "Nintendo of America")

    def test_upsert_region_and_retrieve(self):
        self.db.upsert_reference("regions", {"id": 1, "name": "US"})
        row = self.db.get_reference("regions", 1)
        self.assertEqual(row["name"], "US")

    def test_upsert_country_and_retrieve(self):
        self.db.upsert_reference("countries", {"id": 50, "name": "United States"})
        row = self.db.get_reference("countries", 50)
        self.assertEqual(row["name"], "United States")

    def test_upsert_reference_updates_existing(self):
        self.db.upsert_reference("genres", {"id": 15, "name": "Old Name"})
        self.db.upsert_reference("genres", {"id": 15, "name": "Action"})
        row = self.db.get_reference("genres", 15)
        self.assertEqual(row["name"], "Action")

    def test_upsert_reference_rejects_invalid_table(self):
        with self.assertRaises(ValueError):
            self.db.upsert_reference("evil_table; DROP TABLE games", {"id": 1, "name": "x"})

    def test_get_reference_rejects_invalid_table(self):
        with self.assertRaises(ValueError):
            self.db.get_reference("evil_table", 1)


class TestMetadataDbGames(unittest.TestCase):
    def setUp(self):
        self.db = MetadataDb(":memory:")
        self.db.upsert_platform({"id": 7, "name": "NES", "alias": "nes"})
        self._sample_game = {
            "id": 135,
            "game_title": "Castlevania",
            "release_date": "1987-05-01",
            "platform": 7,
            "region_id": 2,
            "country_id": 50,
            "players": 1,
            "overview": "Fight Dracula.",
            "last_updated": "2025-08-10 23:58:25",
            "rating": "E - Everyone",
            "coop": "No",
            "youtube": "ENSDrPpFp-Y",
            "alternates": None,
            "developers": [476],
            "genres": [15],
            "publishers": [23],
        }

    def tearDown(self):
        self.db.close()

    def test_upsert_game_inserts_new_row(self):
        self.db.upsert_game(self._sample_game)
        game = self.db.get_game(135)
        self.assertIsNotNone(game)
        self.assertEqual(game["game_title"], "Castlevania")

    def test_upsert_game_updates_existing_row(self):
        self.db.upsert_game(self._sample_game)
        updated = dict(self._sample_game)
        updated["game_title"] = "Castlevania Updated"
        self.db.upsert_game(updated)
        game = self.db.get_game(135)
        self.assertEqual(game["game_title"], "Castlevania Updated")

    def test_upsert_game_stores_overview(self):
        self.db.upsert_game(self._sample_game)
        game = self.db.get_game(135)
        self.assertEqual(game["overview"], "Fight Dracula.")

    def test_upsert_game_stores_platform_id(self):
        self.db.upsert_game(self._sample_game)
        game = self.db.get_game(135)
        self.assertEqual(game["platform_id"], 7)

    def test_get_game_returns_none_for_unknown_id(self):
        self.assertIsNone(self.db.get_game(9999))

    def test_upsert_game_creates_genre_join_rows(self):
        self.db.upsert_reference("genres", {"id": 15, "name": "Action"})
        self.db.upsert_game(self._sample_game)
        genres = self.db.get_game_genres(135)
        self.assertEqual(genres, [15])

    def test_upsert_game_creates_developer_join_rows(self):
        self.db.upsert_reference("developers", {"id": 476, "name": "Konami"})
        self.db.upsert_game(self._sample_game)
        devs = self.db.get_game_developers(135)
        self.assertEqual(devs, [476])

    def test_upsert_game_creates_publisher_join_rows(self):
        self.db.upsert_reference("publishers", {"id": 23, "name": "Konami Inc."})
        self.db.upsert_game(self._sample_game)
        pubs = self.db.get_game_publishers(135)
        self.assertEqual(pubs, [23])

    def test_list_games_by_platform(self):
        self.db.upsert_game(self._sample_game)
        games = self.db.list_games(platform_id=7)
        self.assertEqual(len(games), 1)
        self.assertEqual(games[0]["id"], 135)

    def test_list_games_by_platform_excludes_other_platforms(self):
        self.db.upsert_platform({"id": 4, "name": "GB", "alias": "gb"})
        other = dict(self._sample_game)
        other["id"] = 999
        other["platform"] = 4
        self.db.upsert_game(other)
        self.db.upsert_game(self._sample_game)
        games = self.db.list_games(platform_id=4)
        self.assertEqual(len(games), 1)
        self.assertEqual(games[0]["id"], 999)

    def test_list_all_games_returns_all_platforms(self):
        self.db.upsert_platform({"id": 4, "name": "GB", "alias": "gb"})
        other = dict(self._sample_game)
        other["id"] = 999
        other["platform"] = 4
        self.db.upsert_game(other)
        self.db.upsert_game(self._sample_game)
        games = self.db.list_games()
        self.assertEqual(len(games), 2)


class TestMetadataDbImages(unittest.TestCase):
    def setUp(self):
        self.db = MetadataDb(":memory:")
        self.db.upsert_platform({"id": 7, "name": "NES", "alias": "nes"})
        game = {
            "id": 135,
            "game_title": "Castlevania",
            "release_date": "1987-05-01",
            "platform": 7,
            "region_id": 2,
            "country_id": 50,
            "players": 1,
            "overview": "",
            "last_updated": "",
            "rating": "",
            "coop": "",
            "youtube": "",
            "alternates": None,
            "developers": [],
            "genres": [],
            "publishers": [],
        }
        self.db.upsert_game(game)

    def tearDown(self):
        self.db.close()

    def test_upsert_image_stores_filename_and_type(self):
        self.db.upsert_image(
            {
                "id": 718,
                "game_id": 135,
                "type": "boxart",
                "side": "back",
                "filename": "boxart/back/135-2.jpg",
                "resolution": "1000x1435",
            }
        )
        images = self.db.get_game_images(135)
        self.assertEqual(len(images), 1)
        self.assertEqual(images[0]["filename"], "boxart/back/135-2.jpg")
        self.assertEqual(images[0]["type"], "boxart")

    def test_upsert_image_updates_existing_by_id(self):
        self.db.upsert_image(
            {"id": 718, "game_id": 135, "type": "boxart", "side": "back", "filename": "old.jpg", "resolution": None}
        )
        self.db.upsert_image(
            {"id": 718, "game_id": 135, "type": "boxart", "side": "back", "filename": "new.jpg", "resolution": None}
        )
        images = self.db.get_game_images(135)
        self.assertEqual(len(images), 1)
        self.assertEqual(images[0]["filename"], "new.jpg")

    def test_get_game_images_returns_empty_list_for_no_images(self):
        images = self.db.get_game_images(135)
        self.assertEqual(images, [])

    def test_get_game_image_counts_by_type_returns_empty_dict_for_no_images(self):
        counts = self.db.get_game_image_counts_by_type(135)
        self.assertEqual(counts, {})

    def test_get_game_image_counts_by_type_groups_by_type(self):
        self.db.upsert_game({"id": 135, "game_title": "Castlevania", "platform": 7})
        for img_id, img_type in [
            (1, "boxart"),
            (2, "boxart"),
            (3, "screenshot"),
            (4, "screenshot"),
            (5, "screenshot"),
            (6, "fanart"),
        ]:
            self.db.upsert_image(
                {
                    "id": img_id,
                    "game_id": 135,
                    "type": img_type,
                    "side": None,
                    "filename": f"{img_id}.jpg",
                    "resolution": None,
                }
            )
        counts = self.db.get_game_image_counts_by_type(135)
        self.assertEqual(counts, {"boxart": 2, "screenshot": 3, "fanart": 1})

    def test_get_game_image_counts_by_type_handles_null_type(self):
        self.db.upsert_game({"id": 135, "game_title": "Castlevania", "platform": 7})
        self.db.upsert_image(
            {"id": 1, "game_id": 135, "type": None, "side": None, "filename": "1.jpg", "resolution": None}
        )
        self.db.upsert_image(
            {"id": 2, "game_id": 135, "type": "boxart", "side": None, "filename": "2.jpg", "resolution": None}
        )
        counts = self.db.get_game_image_counts_by_type(135)
        self.assertEqual(counts.get("boxart"), 1)
        self.assertEqual(counts.get(None), 1)

    def test_upsert_image_base_urls_stores_all_sizes(self):
        base_urls = {
            "original": "https://cdn.thegamesdb.net/images/original/",
            "small": "https://cdn.thegamesdb.net/images/small/",
            "thumb": "https://cdn.thegamesdb.net/images/thumb/",
            "cropped_center_thumb": "https://cdn.thegamesdb.net/images/cropped_center_thumb/",
            "medium": "https://cdn.thegamesdb.net/images/medium/",
            "large": "https://cdn.thegamesdb.net/images/large/",
        }
        self.db.upsert_image_base_urls(base_urls)
        stored = self.db.get_image_base_urls()
        for size, url in base_urls.items():
            self.assertEqual(stored[size], url)

    def test_build_image_url_combines_base_and_filename(self):
        self.db.upsert_image_base_urls(
            {
                "original": "https://cdn.thegamesdb.net/images/original/",
            }
        )
        self.db.upsert_image(
            {
                "id": 718,
                "game_id": 135,
                "type": "boxart",
                "side": "back",
                "filename": "boxart/back/135-2.jpg",
                "resolution": None,
            }
        )
        url = self.db.build_image_url(718, "original")
        self.assertEqual(url, "https://cdn.thegamesdb.net/images/original/boxart/back/135-2.jpg")

    def test_build_image_url_raises_for_unknown_size(self):
        self.db.upsert_image(
            {
                "id": 718,
                "game_id": 135,
                "type": "boxart",
                "side": "back",
                "filename": "boxart/back/135-2.jpg",
                "resolution": None,
            }
        )
        with self.assertRaises(KeyError):
            self.db.build_image_url(718, "nonexistent_size")


class TestMetadataDbSyncLog(unittest.TestCase):
    def setUp(self):
        self.db = MetadataDb(":memory:")
        self.db.upsert_platform({"id": 7, "name": "NES", "alias": "nes"})

    def tearDown(self):
        self.db.close()

    def test_get_sync_log_returns_none_for_new_platform(self):
        log = self.db.get_sync_log(7)
        self.assertIsNone(log)

    def test_update_sync_log_full_sync(self):
        self.db.update_sync_log(7, last_full_sync="2026-01-01T00:00:00", last_update_id=1000)
        log = self.db.get_sync_log(7)
        self.assertIsNotNone(log)
        self.assertEqual(log["last_full_sync"], "2026-01-01T00:00:00")
        self.assertEqual(log["last_update_id"], 1000)

    def test_update_sync_log_incremental_sync(self):
        self.db.update_sync_log(7, last_full_sync="2026-01-01T00:00:00", last_update_id=1000)
        self.db.update_sync_log(7, last_incremental_sync="2026-02-01T00:00:00", last_update_id=2000)
        log = self.db.get_sync_log(7)
        self.assertEqual(log["last_incremental_sync"], "2026-02-01T00:00:00")
        self.assertEqual(log["last_update_id"], 2000)
        # full sync timestamp should be preserved
        self.assertEqual(log["last_full_sync"], "2026-01-01T00:00:00")

    def test_get_game_count_per_platform(self):
        self.db.upsert_platform({"id": 4, "name": "GB", "alias": "gb"})
        game_nes = {
            "id": 135,
            "game_title": "Castlevania",
            "release_date": "",
            "platform": 7,
            "region_id": 0,
            "country_id": 0,
            "players": 1,
            "overview": "",
            "last_updated": "",
            "rating": "",
            "coop": "",
            "youtube": "",
            "alternates": None,
            "developers": [],
            "genres": [],
            "publishers": [],
        }
        self.db.upsert_game(game_nes)
        counts = self.db.get_game_counts()
        self.assertEqual(counts[7], 1)
        self.assertNotIn(4, counts)


class TestMetadataDbSearchGames(unittest.TestCase):
    """Tests for MetadataDb.search_games — case-insensitive substring lookup."""

    def setUp(self):
        self.db = MetadataDb(":memory:")
        self.db.upsert_platform({"id": 7, "name": "NES", "alias": "nes"})
        self.db.upsert_platform({"id": 4, "name": "GB", "alias": "gb"})
        self._base = {
            "release_date": "",
            "region_id": 0,
            "country_id": 0,
            "players": 1,
            "overview": "",
            "last_updated": "",
            "rating": "",
            "coop": "",
            "youtube": "",
            "alternates": None,
            "developers": [],
            "genres": [],
            "publishers": [],
        }

    def tearDown(self):
        self.db.close()

    def _game(self, game_id, title, platform=7):
        return {"id": game_id, "game_title": title, "platform": platform, **self._base}

    def test_returns_exact_match(self):
        self.db.upsert_game(self._game(135, "Castlevania"))
        results = self.db.search_games("Castlevania")
        self.assertEqual(len(results), 1)
        self.assertEqual(results[0]["id"], 135)

    def test_partial_name_matches_substring(self):
        self.db.upsert_game(self._game(135, "Castlevania"))
        self.db.upsert_game(self._game(136, "Castlevania II - Simon's Quest"))
        results = self.db.search_games("castlevania")
        self.assertEqual(len(results), 2)

    def test_case_insensitive_match(self):
        self.db.upsert_game(self._game(135, "Castlevania"))
        results = self.db.search_games("CASTLE")
        self.assertEqual(len(results), 1)

    def test_no_match_returns_empty_list(self):
        self.db.upsert_game(self._game(135, "Castlevania"))
        results = self.db.search_games("zeldaXXX")
        self.assertEqual(results, [])

    def test_platform_filter_narrows_results(self):
        self.db.upsert_game(self._game(135, "Castlevania", platform=7))
        self.db.upsert_game(self._game(200, "Castlevania II", platform=4))
        results = self.db.search_games("castlevania", platform_id=7)
        self.assertEqual(len(results), 1)
        self.assertEqual(results[0]["id"], 135)

    def test_results_include_game_fields(self):
        self.db.upsert_game(self._game(135, "Castlevania"))
        results = self.db.search_games("Castlevania")
        self.assertIn("id", results[0])
        self.assertIn("game_title", results[0])
        self.assertIn("platform_id", results[0])


if __name__ == "__main__":
    unittest.main()
