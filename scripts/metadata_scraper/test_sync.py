"""Tests for Syncer — MetadataDb and TheGamesDbClient are both mocked."""

import unittest
from unittest.mock import MagicMock, patch

from scripts.metadata_scraper.sync import Syncer

# ── tqdm passthrough helper ───────────────────────────────────────────────────


def _passthrough_tqdm(iterable, **kwargs):
    """Fake tqdm that iterates normally and silently accepts all tqdm method calls."""

    class _PbarProxy:
        def __init__(self, items):
            self._items = items

        def __iter__(self):
            return iter(self._items)

        def __getattr__(self, _):
            return lambda *a, **kw: None

    return _PbarProxy(list(iterable))


# ── shared fixtures ───────────────────────────────────────────────────────────

PLATFORM_NES = {
    "id": 7,
    "name": "Nintendo Entertainment System (NES)",
    "alias": "nintendo-entertainment-system-nes",
    "slug": "nes",
}

SAMPLE_GAME = {
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

BASE_URL = {
    "original": "https://cdn.thegamesdb.net/images/original/",
    "small": "https://cdn.thegamesdb.net/images/small/",
    "thumb": "https://cdn.thegamesdb.net/images/thumb/",
    "cropped_center_thumb": "https://cdn.thegamesdb.net/images/cropped_center_thumb/",
    "medium": "https://cdn.thegamesdb.net/images/medium/",
    "large": "https://cdn.thegamesdb.net/images/large/",
}

SAMPLE_IMAGE = {
    "id": 718,
    "type": "boxart",
    "side": "back",
    "filename": "boxart/back/135-2.jpg",
    "resolution": "1000x1435",
}


def _make_syncer():
    db = MagicMock()
    client = MagicMock()
    return Syncer(db=db, client=client), db, client


class TestSyncerFullSync(unittest.TestCase):
    def test_full_sync_fetches_reference_data(self):
        syncer, db, client = _make_syncer()
        client.get_games_by_platform.return_value = {
            "games": [SAMPLE_GAME],
            "base_url": BASE_URL,
            "boxart": {},
        }
        client.get_games_images.return_value = {
            "images": {"135": [SAMPLE_IMAGE]},
            "base_url": BASE_URL,
        }
        client.get_genres.return_value = {"15": {"id": 15, "name": "Action"}}
        client.get_developers.return_value = {"476": {"id": 476, "name": "Konami"}}
        client.get_publishers.return_value = {"23": {"id": 23, "name": "Konami Inc."}}
        client.get_regions.return_value = {"2": {"id": 2, "name": "Japan"}}
        client.get_countries.return_value = {"50": {"id": 50, "name": "United States"}}
        db.get_sync_log.return_value = None

        syncer.full_sync(platform_id=7, platform_info=PLATFORM_NES)

        client.get_genres.assert_called_once()
        client.get_developers.assert_called_once()
        client.get_publishers.assert_called_once()
        client.get_regions.assert_called_once()
        client.get_countries.assert_called_once()

    def test_full_sync_upserts_platform(self):
        syncer, db, client = _make_syncer()
        client.get_games_by_platform.return_value = {"games": [], "base_url": BASE_URL, "boxart": {}}
        client.get_games_images.return_value = {"images": {}, "base_url": BASE_URL}
        client.get_genres.return_value = {}
        client.get_developers.return_value = {}
        client.get_publishers.return_value = {}
        client.get_regions.return_value = {}
        client.get_countries.return_value = {}
        db.get_sync_log.return_value = None

        syncer.full_sync(platform_id=7, platform_info=PLATFORM_NES)

        db.upsert_platform.assert_called_once_with(PLATFORM_NES)

    def test_full_sync_upserts_every_game(self):
        syncer, db, client = _make_syncer()
        client.get_games_by_platform.return_value = {
            "games": [SAMPLE_GAME],
            "base_url": BASE_URL,
            "boxart": {},
        }
        client.get_games_images.return_value = {"images": {}, "base_url": BASE_URL}
        client.get_genres.return_value = {}
        client.get_developers.return_value = {}
        client.get_publishers.return_value = {}
        client.get_regions.return_value = {}
        client.get_countries.return_value = {}
        db.get_sync_log.return_value = None

        syncer.full_sync(platform_id=7, platform_info=PLATFORM_NES)

        db.upsert_game.assert_called_once()
        call_arg = db.upsert_game.call_args[0][0]
        self.assertEqual(call_arg["id"], 135)

    def test_full_sync_upserts_images(self):
        syncer, db, client = _make_syncer()
        client.get_games_by_platform.return_value = {
            "games": [SAMPLE_GAME],
            "base_url": BASE_URL,
            "boxart": {"135": [SAMPLE_IMAGE]},
        }
        client.get_games_images.return_value = {
            "images": {"135": [SAMPLE_IMAGE]},
            "base_url": BASE_URL,
        }
        client.get_genres.return_value = {}
        client.get_developers.return_value = {}
        client.get_publishers.return_value = {}
        client.get_regions.return_value = {}
        client.get_countries.return_value = {}
        db.get_sync_log.return_value = None

        syncer.full_sync(platform_id=7, platform_info=PLATFORM_NES)

        db.upsert_image.assert_called()

    def test_full_sync_upserts_image_base_urls(self):
        syncer, db, client = _make_syncer()
        client.get_games_by_platform.return_value = {
            "games": [],
            "base_url": BASE_URL,
            "boxart": {},
        }
        client.get_games_images.return_value = {"images": {}, "base_url": BASE_URL}
        client.get_genres.return_value = {}
        client.get_developers.return_value = {}
        client.get_publishers.return_value = {}
        client.get_regions.return_value = {}
        client.get_countries.return_value = {}
        db.get_sync_log.return_value = None

        syncer.full_sync(platform_id=7, platform_info=PLATFORM_NES)

        db.upsert_image_base_urls.assert_called()

    def test_full_sync_updates_sync_log_after_completion(self):
        syncer, db, client = _make_syncer()
        client.get_games_by_platform.return_value = {"games": [], "base_url": BASE_URL, "boxart": {}}
        client.get_games_images.return_value = {"images": {}, "base_url": BASE_URL}
        client.get_genres.return_value = {}
        client.get_developers.return_value = {}
        client.get_publishers.return_value = {}
        client.get_regions.return_value = {}
        client.get_countries.return_value = {}
        db.get_sync_log.return_value = None

        syncer.full_sync(platform_id=7, platform_info=PLATFORM_NES)

        db.update_sync_log.assert_called()
        kwargs = db.update_sync_log.call_args[1]
        self.assertEqual(kwargs["platform_id"], 7)
        self.assertIn("last_full_sync", kwargs)

    def test_full_sync_upserts_reference_entities(self):
        syncer, db, client = _make_syncer()
        client.get_games_by_platform.return_value = {"games": [], "base_url": BASE_URL, "boxart": {}}
        client.get_games_images.return_value = {"images": {}, "base_url": BASE_URL}
        client.get_genres.return_value = {"15": {"id": 15, "name": "Action"}}
        client.get_developers.return_value = {}
        client.get_publishers.return_value = {}
        client.get_regions.return_value = {}
        client.get_countries.return_value = {}
        db.get_sync_log.return_value = None

        syncer.full_sync(platform_id=7, platform_info=PLATFORM_NES)

        db.upsert_reference.assert_any_call("genres", {"id": 15, "name": "Action"})


class TestSyncerIncrementalSync(unittest.TestCase):
    def test_incremental_sync_uses_updates_endpoint(self):
        syncer, db, client = _make_syncer()
        db.get_sync_log.return_value = {
            "last_full_sync": "2026-01-01T00:00:00",
            "last_incremental_sync": None,
            "last_update_id": 1000,
        }
        client.get_games_updates.return_value = {"updates": []}
        client.get_games_by_id.return_value = {"games": []}
        client.get_games_images.return_value = {"images": {}, "base_url": BASE_URL}

        syncer.incremental_sync(platform_id=7, platform_info=PLATFORM_NES)

        client.get_games_updates.assert_called_once()
        call_kwargs = client.get_games_updates.call_args[1]
        self.assertEqual(call_kwargs["last_edit_id"], 1000)

    def test_incremental_sync_only_fetches_updated_games(self):
        syncer, db, client = _make_syncer()
        db.get_sync_log.return_value = {
            "last_full_sync": "2026-01-01T00:00:00",
            "last_incremental_sync": None,
            "last_update_id": 1000,
        }
        client.get_games_updates.return_value = {
            "updates": [{"game_id": 135, "edit_id": 1001}],
        }
        client.get_games_by_id.return_value = {
            "games": [SAMPLE_GAME],
        }
        client.get_games_images.return_value = {"images": {}, "base_url": BASE_URL}

        syncer.incremental_sync(platform_id=7, platform_info=PLATFORM_NES)

        client.get_games_by_id.assert_called_once_with([135])

    def test_incremental_sync_skips_if_no_sync_log(self):
        """If no full sync has been done, incremental should raise or be a no-op."""
        syncer, db, _client = _make_syncer()
        db.get_sync_log.return_value = None

        with self.assertRaises(RuntimeError):
            syncer.incremental_sync(platform_id=7, platform_info=PLATFORM_NES)

    def test_incremental_sync_updates_sync_log(self):
        syncer, db, client = _make_syncer()
        db.get_sync_log.return_value = {
            "last_full_sync": "2026-01-01T00:00:00",
            "last_incremental_sync": None,
            "last_update_id": 1000,
        }
        client.get_games_updates.return_value = {
            "updates": [{"game_id": 135, "edit_id": 1005}],
        }
        client.get_games_by_id.return_value = {"games": [SAMPLE_GAME]}
        client.get_games_images.return_value = {"images": {}, "base_url": BASE_URL}

        syncer.incremental_sync(platform_id=7, platform_info=PLATFORM_NES)

        db.update_sync_log.assert_called()
        kwargs = db.update_sync_log.call_args[1]
        self.assertIn("last_incremental_sync", kwargs)
        self.assertEqual(kwargs["last_update_id"], 1005)

    def test_incremental_sync_with_no_updates_makes_no_game_fetches(self):
        syncer, db, client = _make_syncer()
        db.get_sync_log.return_value = {
            "last_full_sync": "2026-01-01T00:00:00",
            "last_incremental_sync": None,
            "last_update_id": 1000,
        }
        client.get_games_updates.return_value = {"updates": []}

        syncer.incremental_sync(platform_id=7, platform_info=PLATFORM_NES)

        client.get_games_by_id.assert_not_called()


class TestSyncerSyncDispatch(unittest.TestCase):
    def test_sync_calls_full_sync_when_no_prior_log(self):
        syncer, db, _client = _make_syncer()
        db.get_sync_log.return_value = None
        syncer.full_sync = MagicMock()
        syncer.incremental_sync = MagicMock()

        syncer.sync(platform_id=7, platform_info=PLATFORM_NES)

        syncer.full_sync.assert_called_once_with(platform_id=7, platform_info=PLATFORM_NES)
        syncer.incremental_sync.assert_not_called()

    def test_sync_calls_incremental_when_prior_log_exists(self):
        syncer, db, _client = _make_syncer()
        db.get_sync_log.return_value = {
            "last_full_sync": "2026-01-01T00:00:00",
            "last_incremental_sync": None,
            "last_update_id": 1000,
        }
        syncer.full_sync = MagicMock()
        syncer.incremental_sync = MagicMock()

        syncer.sync(platform_id=7, platform_info=PLATFORM_NES)

        syncer.incremental_sync.assert_called_once_with(platform_id=7, platform_info=PLATFORM_NES)
        syncer.full_sync.assert_not_called()

    def test_sync_calls_full_sync_when_force_full(self):
        syncer, db, _client = _make_syncer()
        db.get_sync_log.return_value = {
            "last_full_sync": "2026-01-01T00:00:00",
            "last_incremental_sync": None,
            "last_update_id": 1000,
        }
        syncer.full_sync = MagicMock()
        syncer.incremental_sync = MagicMock()

        syncer.sync(platform_id=7, platform_info=PLATFORM_NES, force_full=True)

        syncer.full_sync.assert_called_once()
        syncer.incremental_sync.assert_not_called()


class TestSyncerProgress(unittest.TestCase):
    """Verify that progress bars are shown when verbose=True."""

    def _make_full_sync_mocks(self):
        db = MagicMock()
        client = MagicMock()
        client.get_games_by_platform.return_value = {
            "games": [SAMPLE_GAME],
            "base_url": BASE_URL,
            "boxart": {},
        }
        client.get_games_images.return_value = {"images": {}, "base_url": BASE_URL}
        client.get_genres.return_value = {}
        client.get_developers.return_value = {}
        client.get_publishers.return_value = {}
        client.get_regions.return_value = {}
        client.get_countries.return_value = {}
        db.get_sync_log.return_value = None
        return db, client

    def test_syncer_accepts_verbose_parameter(self):
        db, client = self._make_full_sync_mocks()
        syncer = Syncer(db=db, client=client, verbose=True)
        self.assertIsNotNone(syncer)

    @patch("scripts.metadata_scraper.sync.tqdm", side_effect=_passthrough_tqdm)
    def test_full_sync_verbose_shows_game_progress_bar(self, mock_tqdm):
        db, client = self._make_full_sync_mocks()
        syncer = Syncer(db=db, client=client, verbose=True)

        syncer.full_sync(platform_id=7, platform_info=PLATFORM_NES)

        descs = [kw.get("desc", "") for _, kw in mock_tqdm.call_args_list]
        self.assertTrue(
            any("game" in d.lower() for d in descs),
            f"Expected a tqdm bar with 'game' in desc, got: {descs}",
        )

    @patch("scripts.metadata_scraper.sync.tqdm", side_effect=_passthrough_tqdm)
    def test_full_sync_verbose_shows_reference_data_progress_bar(self, mock_tqdm):
        db, client = self._make_full_sync_mocks()
        syncer = Syncer(db=db, client=client, verbose=True)

        syncer.full_sync(platform_id=7, platform_info=PLATFORM_NES)

        descs = [kw.get("desc", "") for _, kw in mock_tqdm.call_args_list]
        self.assertTrue(
            any("reference" in d.lower() for d in descs),
            f"Expected a tqdm bar with 'reference' in desc, got: {descs}",
        )

    @patch("scripts.metadata_scraper.sync.tqdm", side_effect=_passthrough_tqdm)
    def test_full_sync_verbose_shows_image_progress_bar(self, mock_tqdm):
        db, client = self._make_full_sync_mocks()
        syncer = Syncer(db=db, client=client, verbose=True)

        syncer.full_sync(platform_id=7, platform_info=PLATFORM_NES)

        descs = [kw.get("desc", "") for _, kw in mock_tqdm.call_args_list]
        self.assertTrue(
            any("image" in d.lower() for d in descs),
            f"Expected a tqdm bar with 'image' in desc, got: {descs}",
        )

    @patch("scripts.metadata_scraper.sync.tqdm", side_effect=_passthrough_tqdm)
    def test_full_sync_not_verbose_passes_disable_true_to_tqdm(self, mock_tqdm):
        db, client = self._make_full_sync_mocks()
        syncer = Syncer(db=db, client=client, verbose=False)

        syncer.full_sync(platform_id=7, platform_info=PLATFORM_NES)

        for c in mock_tqdm.call_args_list:
            self.assertTrue(
                c.kwargs.get("disable", False),
                f"Expected disable=True when verbose=False, got: {c}",
            )

    @patch("scripts.metadata_scraper.sync.tqdm", side_effect=_passthrough_tqdm)
    def test_incremental_sync_verbose_shows_game_progress_bar(self, mock_tqdm):
        db = MagicMock()
        client = MagicMock()
        db.get_sync_log.return_value = {
            "last_full_sync": "2026-01-01T00:00:00",
            "last_incremental_sync": None,
            "last_update_id": 1000,
        }
        client.get_games_updates.return_value = {
            "updates": [{"game_id": 135, "edit_id": 1001}],
        }
        client.get_games_by_id.return_value = {"games": [SAMPLE_GAME]}
        client.get_games_images.return_value = {"images": {}, "base_url": BASE_URL}

        syncer = Syncer(db=db, client=client, verbose=True)
        syncer.incremental_sync(platform_id=7, platform_info=PLATFORM_NES)

        descs = [kw.get("desc", "") for _, kw in mock_tqdm.call_args_list]
        self.assertTrue(
            any("game" in d.lower() for d in descs),
            f"Expected a tqdm bar with 'game' in desc, got: {descs}",
        )


if __name__ == "__main__":
    unittest.main()
