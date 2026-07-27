"""Tests for TheGamesDbClient — all HTTP is mocked, no real network calls."""
import os
import sys
import unittest
from unittest.mock import MagicMock, patch, call

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from api_client import TheGamesDbClient, ApiError

FAKE_KEY = "fake_api_key"

# ── helpers ──────────────────────────────────────────────────────────────────

def _ok(data: dict) -> MagicMock:
    """Return a mock requests.Response with status 200 and the given JSON."""
    resp = MagicMock()
    resp.status_code = 200
    resp.json.return_value = {"code": 200, "status": "Success", "data": data,
                              "remaining_monthly_allowance": 900,
                              "extra_allowance": 0}
    return resp


def _page(games: list, page: int = 1, total: int = None) -> MagicMock:
    total = total or len(games)
    resp = MagicMock()
    resp.status_code = 200
    resp.json.return_value = {
        "code": 200,
        "status": "Success",
        "data": {"count": len(games), "games": games},
        "pages": {
            "previous": None,
            "current": f"https://api.thegamesdb.net/v1/Games/ByPlatformID?page={page}",
            "next": None if len(games) < 20 else f"https://api.thegamesdb.net/v1/Games/ByPlatformID?page={page+1}",
        },
        "remaining_monthly_allowance": 900,
        "extra_allowance": 0,
    }
    return resp


# ── construction ─────────────────────────────────────────────────────────────

class TestTheGamesDbClientConstruction(unittest.TestCase):
    def test_raises_if_no_api_key(self):
        with self.assertRaises(ValueError):
            TheGamesDbClient(api_key="")

    def test_accepts_valid_api_key(self):
        client = TheGamesDbClient(api_key=FAKE_KEY)
        self.assertIsNotNone(client)


# ── rate limiting ─────────────────────────────────────────────────────────────

class TestTheGamesDbClientRateLimiting(unittest.TestCase):
    @patch("api_client.time.sleep")
    @patch("api_client.requests.get")
    def test_sleep_called_between_requests(self, mock_get, mock_sleep):
        game = {"id": 5, "game_title": "Donkey Kong", "release_date": "1986-06-01",
                "platform": 7, "region_id": 2, "country_id": 50}
        mock_get.return_value = _page([game])
        client = TheGamesDbClient(api_key=FAKE_KEY, request_delay=0.2)
        client.get_games_by_platform(7)
        client.get_games_by_platform(7)
        self.assertGreaterEqual(mock_sleep.call_count, 1)

    @patch("api_client.time.sleep")
    @patch("api_client.requests.get")
    def test_sleep_duration_matches_configured_delay(self, mock_get, mock_sleep):
        mock_get.return_value = _page([])
        client = TheGamesDbClient(api_key=FAKE_KEY, request_delay=0.123)
        client.get_games_by_platform(7)
        client.get_games_by_platform(7)
        for c in mock_sleep.call_args_list:
            self.assertAlmostEqual(c.args[0], 0.123, places=2)


# ── get_games_by_platform ─────────────────────────────────────────────────────

class TestGetGamesByPlatform(unittest.TestCase):
    @patch("api_client.time.sleep")
    @patch("api_client.requests.get")
    def test_returns_games_from_single_page(self, mock_get, _sleep):
        games = [{"id": 5, "game_title": "Donkey Kong", "release_date": "1986-06-01",
                  "platform": 7, "region_id": 2, "country_id": 50}]
        mock_get.return_value = _page(games)
        client = TheGamesDbClient(api_key=FAKE_KEY)
        result = client.get_games_by_platform(7)
        self.assertEqual(len(result["games"]), 1)
        self.assertEqual(result["games"][0]["id"], 5)

    @patch("api_client.time.sleep")
    @patch("api_client.requests.get")
    def test_paginates_through_all_pages(self, mock_get, _sleep):
        page1_games = [{"id": i, "game_title": f"Game {i}", "release_date": "",
                        "platform": 7, "region_id": 0, "country_id": 0} for i in range(20)]
        page2_games = [{"id": 20, "game_title": "Game 20", "release_date": "",
                        "platform": 7, "region_id": 0, "country_id": 0}]
        mock_get.side_effect = [_page(page1_games, page=1, total=21),
                                 _page(page2_games, page=2, total=21)]
        client = TheGamesDbClient(api_key=FAKE_KEY)
        result = client.get_games_by_platform(7)
        self.assertEqual(len(result["games"]), 21)
        self.assertEqual(mock_get.call_count, 2)

    @patch("api_client.time.sleep")
    @patch("api_client.requests.get")
    def test_includes_all_fields_parameter(self, mock_get, _sleep):
        mock_get.return_value = _page([])
        client = TheGamesDbClient(api_key=FAKE_KEY)
        client.get_games_by_platform(7)
        url = mock_get.call_args[0][0]
        self.assertIn("fields=", url)
        self.assertIn("overview", url)

    @patch("api_client.time.sleep")
    @patch("api_client.requests.get")
    def test_includes_boxart_in_include_parameter(self, mock_get, _sleep):
        mock_get.return_value = _page([])
        client = TheGamesDbClient(api_key=FAKE_KEY)
        client.get_games_by_platform(7)
        url = mock_get.call_args[0][0]
        self.assertIn("include=", url)
        self.assertIn("boxart", url)

    @patch("api_client.time.sleep")
    @patch("api_client.requests.get")
    def test_raises_api_error_on_http_error(self, mock_get, _sleep):
        resp = MagicMock()
        resp.status_code = 429
        resp.json.return_value = {"code": 429, "status": "Too Many Requests"}
        mock_get.return_value = resp
        client = TheGamesDbClient(api_key=FAKE_KEY)
        with self.assertRaises(ApiError):
            client.get_games_by_platform(7)

    @patch("api_client.time.sleep")
    @patch("api_client.requests.get")
    def test_api_error_carries_http_status_code(self, mock_get, _sleep):
        resp = MagicMock()
        resp.status_code = 429
        resp.json.return_value = {"code": 429, "status": "Too Many Requests"}
        mock_get.return_value = resp
        client = TheGamesDbClient(api_key=FAKE_KEY)
        with self.assertRaises(ApiError) as ctx:
            client.get_games_by_platform(7)
        self.assertEqual(ctx.exception.status_code, 429)

    @patch("api_client.time.sleep")
    @patch("api_client.requests.get")
    def test_api_error_status_code_is_none_for_invalid_json(self, mock_get, _sleep):
        resp = MagicMock()
        resp.status_code = 200
        resp.json.side_effect = ValueError("not json")
        mock_get.return_value = resp
        client = TheGamesDbClient(api_key=FAKE_KEY)
        with self.assertRaises(ApiError) as ctx:
            client.get_games_by_platform(7)
        self.assertIsNone(ctx.exception.status_code)

    @patch("api_client.time.sleep")
    @patch("api_client.requests.get")
    def test_returns_base_url_when_boxart_present(self, mock_get, _sleep):
        resp = MagicMock()
        resp.status_code = 200
        resp.json.return_value = {
            "code": 200, "status": "Success",
            "data": {"count": 0, "games": []},
            "include": {
                "boxart": {
                    "base_url": {"original": "https://cdn.thegamesdb.net/images/original/"},
                    "data": {},
                }
            },
            "pages": {"previous": None, "current": "...", "next": None},
            "remaining_monthly_allowance": 900,
            "extra_allowance": 0,
        }
        mock_get.return_value = resp
        client = TheGamesDbClient(api_key=FAKE_KEY)
        result = client.get_games_by_platform(7)
        self.assertIn("base_url", result)
        self.assertIn("original", result["base_url"])


# ── get_games_by_id ───────────────────────────────────────────────────────────

class TestGetGamesById(unittest.TestCase):
    @patch("api_client.time.sleep")
    @patch("api_client.requests.get")
    def test_fetches_multiple_ids_in_one_call(self, mock_get, _sleep):
        resp = MagicMock()
        resp.status_code = 200
        resp.json.return_value = {
            "code": 200, "status": "Success",
            "data": {"count": 2, "games": {
                "135": {"id": 135, "game_title": "Castlevania"},
                "140": {"id": 140, "game_title": "Super Mario Bros."},
            }},
            "remaining_monthly_allowance": 899,
            "extra_allowance": 0,
        }
        mock_get.return_value = resp
        client = TheGamesDbClient(api_key=FAKE_KEY)
        result = client.get_games_by_id([135, 140])
        self.assertEqual(len(result["games"]), 2)
        self.assertEqual(mock_get.call_count, 1)

    @patch("api_client.time.sleep")
    @patch("api_client.requests.get")
    def test_batches_large_id_lists(self, mock_get, _sleep):
        """When more than BATCH_SIZE ids, splits into multiple requests."""
        resp = MagicMock()
        resp.status_code = 200
        resp.json.return_value = {
            "code": 200, "status": "Success",
            "data": {"count": 0, "games": {}},
            "remaining_monthly_allowance": 800,
            "extra_allowance": 0,
        }
        mock_get.return_value = resp
        client = TheGamesDbClient(api_key=FAKE_KEY, batch_size=10)
        client.get_games_by_id(list(range(25)))
        self.assertEqual(mock_get.call_count, 3)  # 10+10+5

    @patch("api_client.time.sleep")
    @patch("api_client.requests.get")
    def test_handles_list_response_from_api(self, mock_get, _sleep):
        """ByGameID may return games as a list instead of a dict."""
        resp = MagicMock()
        resp.status_code = 200
        resp.json.return_value = {
            "code": 200, "status": "Success",
            "data": {"count": 2, "games": [
                {"id": 135, "game_title": "Castlevania"},
                {"id": 140, "game_title": "Super Mario Bros."},
            ]},
            "remaining_monthly_allowance": 899,
            "extra_allowance": 0,
        }
        mock_get.return_value = resp
        client = TheGamesDbClient(api_key=FAKE_KEY)
        result = client.get_games_by_id([135, 140])
        self.assertEqual(len(result["games"]), 2)


# ── get_games_images ──────────────────────────────────────────────────────────

class TestGetGamesImages(unittest.TestCase):
    @patch("api_client.time.sleep")
    @patch("api_client.requests.get")
    def test_returns_images_for_given_ids(self, mock_get, _sleep):
        resp = MagicMock()
        resp.status_code = 200
        resp.json.return_value = {
            "code": 200, "status": "Success",
            "data": {
                "count": 1,
                "base_url": {"original": "https://cdn.thegamesdb.net/images/original/"},
                "images": {
                    "135": [{"id": 718, "type": "boxart", "side": "back",
                              "filename": "boxart/back/135-2.jpg", "resolution": "1000x1435"}],
                },
            },
            "remaining_monthly_allowance": 890,
            "extra_allowance": 0,
        }
        mock_get.return_value = resp
        client = TheGamesDbClient(api_key=FAKE_KEY)
        result = client.get_games_images([135])
        self.assertIn("images", result)
        self.assertIn("135", result["images"])

    @patch("api_client.time.sleep")
    @patch("api_client.requests.get")
    def test_batches_large_id_lists(self, mock_get, _sleep):
        resp = MagicMock()
        resp.status_code = 200
        resp.json.return_value = {
            "code": 200, "status": "Success",
            "data": {"count": 0, "base_url": {}, "images": {}},
            "remaining_monthly_allowance": 800,
            "extra_allowance": 0,
        }
        mock_get.return_value = resp
        client = TheGamesDbClient(api_key=FAKE_KEY, batch_size=10)
        client.get_games_images(list(range(25)))
        self.assertEqual(mock_get.call_count, 3)

    @patch("api_client.time.sleep")
    @patch("api_client.requests.get")
    def test_paginates_images_within_a_batch(self, mock_get, _sleep):
        """Images endpoint may return multiple pages; all should be fetched."""
        base_url = {"original": "https://cdn.thegamesdb.net/images/original/"}
        page1 = MagicMock()
        page1.status_code = 200
        page1.json.return_value = {
            "code": 200, "status": "Success",
            "data": {
                "count": 2,
                "base_url": base_url,
                "images": {
                    "135": [{"id": 1, "type": "boxart", "side": "front",
                              "filename": "boxart/front/135-1.jpg", "resolution": None}],
                },
            },
            "pages": {
                "previous": None,
                "current": "https://api.thegamesdb.net/v1/Games/Images?games_id=135&page=1",
                "next": "https://api.thegamesdb.net/v1/Games/Images?games_id=135&page=2",
            },
            "remaining_monthly_allowance": 890,
        }
        page2 = MagicMock()
        page2.status_code = 200
        page2.json.return_value = {
            "code": 200, "status": "Success",
            "data": {
                "count": 2,
                "base_url": base_url,
                "images": {
                    "135": [{"id": 2, "type": "screenshot", "side": None,
                              "filename": "screenshots/135-1.jpg", "resolution": None}],
                },
            },
            "pages": {
                "previous": "https://api.thegamesdb.net/v1/Games/Images?games_id=135&page=1",
                "current": "https://api.thegamesdb.net/v1/Games/Images?games_id=135&page=2",
                "next": None,
            },
            "remaining_monthly_allowance": 889,
        }
        mock_get.side_effect = [page1, page2]
        client = TheGamesDbClient(api_key=FAKE_KEY)
        result = client.get_games_images([135])
        self.assertEqual(mock_get.call_count, 2)
        images_135 = result["images"]["135"]
        ids = {img["id"] for img in images_135}
        self.assertEqual(ids, {1, 2})

    @patch("api_client.time.sleep")
    @patch("api_client.requests.get")
    def test_merges_images_across_pages_for_same_game(self, mock_get, _sleep):
        """Images for the same game id on different pages must be merged, not overwritten."""
        base_url = {"original": "https://cdn.thegamesdb.net/images/original/"}
        def _img_page(img_id, img_type, has_next):
            return {
                "code": 200, "status": "Success",
                "data": {
                    "count": 1, "base_url": base_url,
                    "images": {
                        "135": [{"id": img_id, "type": img_type, "side": None,
                                  "filename": f"{img_type}/{img_id}.jpg", "resolution": None}],
                    },
                },
                "pages": {"previous": None, "current": "...",
                          "next": "...?page=2" if has_next else None},
                "remaining_monthly_allowance": 890,
            }
        resp1, resp2 = MagicMock(), MagicMock()
        resp1.status_code = resp2.status_code = 200
        resp1.json.return_value = _img_page(10, "fanart", has_next=True)
        resp2.json.return_value = _img_page(11, "clearlogo", has_next=False)
        mock_get.side_effect = [resp1, resp2]
        client = TheGamesDbClient(api_key=FAKE_KEY)
        result = client.get_games_images([135])
        images_135 = result["images"]["135"]
        ids = {img["id"] for img in images_135}
        self.assertEqual(ids, {10, 11})


# ── get_games_updates ─────────────────────────────────────────────────────────

class TestGetGamesUpdates(unittest.TestCase):
    @patch("api_client.time.sleep")
    @patch("api_client.requests.get")
    def test_returns_list_of_updated_game_ids(self, mock_get, _sleep):
        resp = MagicMock()
        resp.status_code = 200
        resp.json.return_value = {
            "code": 200, "status": "Success",
            "data": {
                "count": 2,
                "updates": [
                    {"game_id": 135, "edit_id": 1001},
                    {"game_id": 140, "edit_id": 1002},
                ],
            },
            "remaining_monthly_allowance": 850,
            "extra_allowance": 0,
        }
        mock_get.return_value = resp
        client = TheGamesDbClient(api_key=FAKE_KEY)
        result = client.get_games_updates(last_edit_id=999)
        self.assertIn("updates", result)
        self.assertEqual(len(result["updates"]), 2)


# ── reference data endpoints ──────────────────────────────────────────────────

class TestReferenceEndpoints(unittest.TestCase):
    @patch("api_client.time.sleep")
    @patch("api_client.requests.get")
    def test_get_genres_returns_dict_keyed_by_id(self, mock_get, _sleep):
        mock_get.return_value = _ok({
            "count": 1,
            "genres": {"15": {"id": 15, "name": "Action"}},
        })
        client = TheGamesDbClient(api_key=FAKE_KEY)
        result = client.get_genres()
        self.assertIn("15", result)
        self.assertEqual(result["15"]["name"], "Action")

    @patch("api_client.time.sleep")
    @patch("api_client.requests.get")
    def test_get_developers_returns_dict_keyed_by_id(self, mock_get, _sleep):
        mock_get.return_value = _ok({
            "count": 1,
            "developers": {"389": {"id": 389, "name": "Nintendo"}},
        })
        client = TheGamesDbClient(api_key=FAKE_KEY)
        result = client.get_developers()
        self.assertIn("389", result)

    @patch("api_client.time.sleep")
    @patch("api_client.requests.get")
    def test_get_publishers_returns_dict_keyed_by_id(self, mock_get, _sleep):
        mock_get.return_value = _ok({
            "count": 1,
            "publishers": {"252": {"id": 252, "name": "Nintendo of America"}},
        })
        client = TheGamesDbClient(api_key=FAKE_KEY)
        result = client.get_publishers()
        self.assertIn("252", result)

    @patch("api_client.time.sleep")
    @patch("api_client.requests.get")
    def test_get_regions_returns_list(self, mock_get, _sleep):
        mock_get.return_value = _ok({
            "count": 1,
            "regions": {"1": {"id": 1, "name": "US"}},
        })
        client = TheGamesDbClient(api_key=FAKE_KEY)
        result = client.get_regions()
        self.assertIn("1", result)

    @patch("api_client.time.sleep")
    @patch("api_client.requests.get")
    def test_get_countries_returns_list(self, mock_get, _sleep):
        mock_get.return_value = _ok({
            "count": 1,
            "countries": {"50": {"id": 50, "name": "United States"}},
        })
        client = TheGamesDbClient(api_key=FAKE_KEY)
        result = client.get_countries()
        self.assertIn("50", result)


# ── get_api_limit ─────────────────────────────────────────────────────────────

class TestGetApiLimit(unittest.TestCase):
    @patch("api_client.time.sleep")
    @patch("api_client.requests.get")
    def test_returns_remaining_allowance(self, mock_get, _sleep):
        # Unlike every other endpoint, /v1/API/Limit has no "data" wrapper:
        # the allowance fields sit at the top level of the response (see
        # API/include/routes.php in TheGamesDB/TheGamesDBv2).
        resp = MagicMock()
        resp.status_code = 200
        resp.json.return_value = {
            "remaining_monthly_allowance": 750,
            "extra_allowance": 5,
            "allowance_refresh_timer": 1728000,
        }
        mock_get.return_value = resp
        client = TheGamesDbClient(api_key=FAKE_KEY)
        result = client.get_api_limit()
        self.assertEqual(result.get("remaining_monthly_allowance"), 750)
        self.assertEqual(result.get("extra_allowance"), 5)

    @patch("api_client.time.sleep")
    @patch("api_client.requests.get")
    def test_returns_allowance_when_exhausted(self, mock_get, _sleep):
        # When the allowance is used up the server omits extra_allowance.
        resp = MagicMock()
        resp.status_code = 200
        resp.json.return_value = {
            "remaining_monthly_allowance": 0,
            "allowance_refresh_timer": 86400,
        }
        mock_get.return_value = resp
        client = TheGamesDbClient(api_key=FAKE_KEY)
        result = client.get_api_limit()
        self.assertEqual(result.get("remaining_monthly_allowance"), 0)


# ── _paginate progress bar ────────────────────────────────────────────────────

class TestPaginateProgress(unittest.TestCase):
    @patch("api_client.time.sleep")
    @patch("api_client.requests.get")
    def test_progress_bar_shows_page_and_game_count(self, mock_get, _sleep):
        """_paginate updates the bar once per page with accumulated game count."""
        page1_games = [{"id": i} for i in range(20)]
        page2_games = [{"id": 20}]
        mock_get.side_effect = [_page(page1_games, page=1, total=21),
                                 _page(page2_games, page=2, total=21)]

        postfixes = []

        class _FakeBar:
            def update(self, n=1): pass
            def set_postfix_str(self, s, **kw): postfixes.append(s)
            def close(self): pass

        client = TheGamesDbClient(api_key=FAKE_KEY, verbose=True)
        with patch("api_client.tqdm", return_value=_FakeBar()):
            client._paginate("/v1/Games/ByPlatformID", {"id": 7})

        self.assertEqual(postfixes[0], "20 games fetched")
        self.assertEqual(postfixes[1], "21 games fetched")


if __name__ == "__main__":
    unittest.main()
