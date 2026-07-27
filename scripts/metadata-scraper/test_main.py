"""Tests for the CLI entrypoint in main.py."""
import os
import sys
import unittest
from io import StringIO
from unittest.mock import MagicMock, patch

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

class TestMainCliArgParsing(unittest.TestCase):
    """Verify argument parsing without invoking real DB or network."""

    def _run(self, args):
        """Run main.main() with the given argv list and capture stdout."""
        with patch("sys.argv", ["main.py"] + args):
            with patch("main.MetadataDb") as MockDb, \
                 patch("main.TheGamesDbClient") as MockClient, \
                 patch("main.Syncer") as MockSyncer:
                mock_db_instance = MagicMock()
                mock_client_instance = MagicMock()
                mock_syncer_instance = MagicMock()
                MockDb.return_value.__enter__ = MagicMock(return_value=mock_db_instance)
                MockDb.return_value.__exit__ = MagicMock(return_value=False)
                MockClient.return_value = mock_client_instance
                MockSyncer.return_value = mock_syncer_instance
                mock_db_instance.get_game_counts.return_value = {}
                mock_client_instance.get_api_limit.return_value = {
                    "remaining_monthly_allowance": 900, "extra_allowance": 0
                }
                mock_db_instance.list_games.return_value = []
                mock_db_instance.get_game_images.return_value = []
                mock_db_instance.get_image_base_urls.return_value = {}
                captured = StringIO()
                import main as m
                m.main(output=captured)
                return mock_db_instance, mock_client_instance, mock_syncer_instance, captured.getvalue()

    def test_sync_nes_calls_syncer_sync(self):
        _, _, syncer, _ = self._run(["sync", "--platform", "nes", "--api-key", "testkey"])
        syncer.sync.assert_called_once()
        call_kwargs = syncer.sync.call_args[1]
        self.assertEqual(call_kwargs["platform_id"], 7)

    def test_sync_snes_calls_syncer_sync(self):
        _, _, syncer, _ = self._run(["sync", "--platform", "snes", "--api-key", "testkey"])
        syncer.sync.assert_called_once()
        call_kwargs = syncer.sync.call_args[1]
        self.assertEqual(call_kwargs["platform_id"], 6)

    def test_sync_force_full_passes_flag(self):
        _, _, syncer, _ = self._run(["sync", "--platform", "nes", "--force-full", "--api-key", "testkey"])
        call_kwargs = syncer.sync.call_args[1]
        self.assertTrue(call_kwargs["force_full"])

    def test_sync_without_api_key_raises(self):
        with patch("sys.argv", ["main.py", "sync", "--platform", "nes"]), \
             patch.dict("os.environ", {}, clear=True):
            import main as m
            with self.assertRaises(SystemExit):
                m.main(output=StringIO())

    def test_sync_reads_api_key_from_env(self):
        with patch("sys.argv", ["main.py", "sync", "--platform", "nes"]), \
             patch.dict("os.environ", {"THEGAMESDB_API_KEY": "envkey"}), \
             patch("main.MetadataDb") as MockDb, \
             patch("main.TheGamesDbClient") as MockClient, \
             patch("main.Syncer") as MockSyncer:
            mock_db_instance = MagicMock()
            MockDb.return_value.__enter__ = MagicMock(return_value=mock_db_instance)
            MockDb.return_value.__exit__ = MagicMock(return_value=False)
            MockClient.return_value = MagicMock()
            MockSyncer.return_value = MagicMock()
            import main as m
            m.main(output=StringIO())
            MockClient.assert_called_once()
            init_kwargs = MockClient.call_args[1]
            self.assertEqual(init_kwargs["api_key"], "envkey")

    def test_sync_cli_api_key_overrides_env(self):
        with patch("sys.argv", ["main.py", "sync", "--platform", "nes", "--api-key", "clikey"]), \
             patch.dict("os.environ", {"THEGAMESDB_API_KEY": "envkey"}), \
             patch("main.MetadataDb") as MockDb, \
             patch("main.TheGamesDbClient") as MockClient, \
             patch("main.Syncer") as MockSyncer:
            mock_db_instance = MagicMock()
            MockDb.return_value.__enter__ = MagicMock(return_value=mock_db_instance)
            MockDb.return_value.__exit__ = MagicMock(return_value=False)
            MockClient.return_value = MagicMock()
            MockSyncer.return_value = MagicMock()
            import main as m
            m.main(output=StringIO())
            init_kwargs = MockClient.call_args[1]
            self.assertEqual(init_kwargs["api_key"], "clikey")

    def test_list_platform_calls_list_games(self):
        db, _, _, _ = self._run(["list", "--platform", "nes", "--api-key", "testkey"])
        db.list_games.assert_called_once()
        call_kwargs = db.list_games.call_args[1]
        self.assertEqual(call_kwargs["platform_id"], 7)

    def test_list_platform_snes_filters_by_platform_id_6(self):
        db, _, _, _ = self._run(["list", "--platform", "snes", "--api-key", "testkey"])
        db.list_games.assert_called_once()
        call_kwargs = db.list_games.call_args[1]
        self.assertEqual(call_kwargs["platform_id"], 6)

    def test_list_game_id_filters_by_id(self):
        db, _, _, _ = self._run(["list", "--game-id", "135", "--api-key", "testkey"])
        db.list_games.assert_called_once()
        call_kwargs = db.list_games.call_args[1]
        self.assertEqual(call_kwargs["game_id"], 135)

    def test_status_command_prints_game_counts(self):
        with patch("sys.argv", ["main.py", "status", "--api-key", "testkey"]), \
             patch("main.MetadataDb") as MockDb, \
             patch("main.TheGamesDbClient") as MockClient, \
             patch("main.Syncer"):
            mock_db_instance = MagicMock()
            MockDb.return_value.__enter__ = MagicMock(return_value=mock_db_instance)
            MockDb.return_value.__exit__ = MagicMock(return_value=False)
            mock_db_instance.get_game_counts.return_value = {7: 42}
            mock_db_instance.list_platforms.return_value = [
                {"id": 7, "name": "Nintendo Entertainment System (NES)", "alias": "nes"}
            ]
            MockClient.return_value.get_api_limit.return_value = {
                "remaining_monthly_allowance": 900, "extra_allowance": 5
            }
            captured = StringIO()
            import main as m
            m.main(output=captured)
            output = captured.getvalue()
            self.assertIn("42", output)

    def test_images_command_prints_urls(self):
        with patch("sys.argv", ["main.py", "images", "135", "--api-key", "testkey"]), \
             patch("main.MetadataDb") as MockDb, \
             patch("main.TheGamesDbClient"), \
             patch("main.Syncer"):
            mock_db_instance = MagicMock()
            MockDb.return_value.__enter__ = MagicMock(return_value=mock_db_instance)
            MockDb.return_value.__exit__ = MagicMock(return_value=False)
            mock_db_instance.get_game_images.return_value = [
                {"id": 718, "type": "boxart", "side": "back",
                 "filename": "boxart/back/135-2.jpg", "resolution": "1000x1435"}
            ]
            mock_db_instance.build_image_url.return_value = (
                "https://cdn.thegamesdb.net/images/original/boxart/back/135-2.jpg"
            )
            captured = StringIO()
            import main as m
            m.main(output=captured)
            output = captured.getvalue()
            self.assertIn("boxart/back/135-2.jpg", output)

    def test_sync_all_platforms_syncs_nes_gb_gbc_gba_snes(self):
        _, _, syncer, _ = self._run(["sync", "--platform", "all", "--api-key", "testkey"])
        platform_ids = [c[1]["platform_id"] for c in syncer.sync.call_args_list]
        self.assertIn(7, platform_ids)   # NES
        self.assertIn(4, platform_ids)   # GB
        self.assertIn(41, platform_ids)  # GBC
        self.assertIn(5, platform_ids)   # GBA
        self.assertIn(6, platform_ids)   # SNES


class TestInfoCommand(unittest.TestCase):
    """Tests for the 'info' subcommand."""

    def _run_info(self, args, search_results=None):
        search_results = search_results or []
        with patch("sys.argv", ["main.py", "info"] + args), \
             patch("main.MetadataDb") as MockDb, \
             patch("main.TheGamesDbClient"), \
             patch("main.Syncer"):
            mock_db = MagicMock()
            MockDb.return_value.__enter__ = MagicMock(return_value=mock_db)
            MockDb.return_value.__exit__ = MagicMock(return_value=False)
            mock_db.search_games.return_value = search_results
            mock_db.get_game_genres.return_value = []
            mock_db.get_game_developers.return_value = []
            mock_db.get_game_publishers.return_value = []
            mock_db.get_game_images.return_value = []
            mock_db.get_reference.return_value = None
            captured = StringIO()
            import main as m
            m.main(output=captured)
            return mock_db, captured.getvalue()

    def test_info_calls_search_games_with_name(self):
        db, _ = self._run_info(["castlevania"])
        db.search_games.assert_called_once()
        call_args = db.search_games.call_args
        args_list = list(call_args[0]) + list(call_args[1].values())
        self.assertIn("castlevania", args_list)

    def test_info_with_platform_passes_platform_id(self):
        db, _ = self._run_info(["castlevania", "--platform", "nes"])
        call_kwargs = db.search_games.call_args[1]
        self.assertEqual(call_kwargs.get("platform_id"), 7)

    def test_info_with_platform_snes_passes_platform_id_6(self):
        db, _ = self._run_info(["mario", "--platform", "snes"])
        call_kwargs = db.search_games.call_args[1]
        self.assertEqual(call_kwargs.get("platform_id"), 6)

    def test_info_no_results_prints_not_found(self):
        _, output = self._run_info(["zeldaXXX"])
        self.assertIn("No games found", output)
        self.assertIn("zeldaXXX", output)

    def test_info_prints_game_title_and_id(self):
        game = {
            "id": 135, "game_title": "Castlevania", "platform_id": 7,
            "release_date": "1987-05-01", "rating": "E", "players": 1,
            "overview": "Fight Dracula.", "coop": "No", "youtube": "",
            "alternates": None, "last_updated": "2025-01-01",
        }
        _, output = self._run_info(["castlevania"], search_results=[game])
        self.assertIn("135", output)
        self.assertIn("Castlevania", output)

    def test_info_resolves_genre_names(self):
        game = {
            "id": 135, "game_title": "Castlevania", "platform_id": 7,
            "release_date": "", "rating": "", "players": None,
            "overview": "", "coop": "", "youtube": "",
            "alternates": None, "last_updated": "",
        }
        with patch("sys.argv", ["main.py", "info", "castlevania"]), \
             patch("main.MetadataDb") as MockDb, \
             patch("main.TheGamesDbClient"), \
             patch("main.Syncer"):
            mock_db = MagicMock()
            MockDb.return_value.__enter__ = MagicMock(return_value=mock_db)
            MockDb.return_value.__exit__ = MagicMock(return_value=False)
            mock_db.search_games.return_value = [game]
            mock_db.get_game_genres.return_value = [15]
            mock_db.get_game_developers.return_value = []
            mock_db.get_game_publishers.return_value = []
            mock_db.get_game_images.return_value = []
            mock_db.get_reference.side_effect = lambda table, eid: (
                {"id": 15, "name": "Action"} if table == "genres" and eid == 15 else None
            )
            captured = StringIO()
            import main as m
            m.main(output=captured)
            self.assertIn("Action", captured.getvalue())

    def test_info_shows_image_count(self):
        game = {
            "id": 135, "game_title": "Castlevania", "platform_id": 7,
            "release_date": "", "rating": "", "players": None,
            "overview": "", "coop": "", "youtube": "",
            "alternates": None, "last_updated": "",
        }
        with patch("sys.argv", ["main.py", "info", "castlevania"]), \
             patch("main.MetadataDb") as MockDb, \
             patch("main.TheGamesDbClient"), \
             patch("main.Syncer"):
            mock_db = MagicMock()
            MockDb.return_value.__enter__ = MagicMock(return_value=mock_db)
            MockDb.return_value.__exit__ = MagicMock(return_value=False)
            mock_db.search_games.return_value = [game]
            mock_db.get_game_genres.return_value = []
            mock_db.get_game_developers.return_value = []
            mock_db.get_game_publishers.return_value = []
            mock_db.get_game_image_counts_by_type.return_value = {
                "boxart": 2, "screenshot": 3, "fanart": 1,
            }
            mock_db.get_reference.return_value = None
            captured = StringIO()
            import main as m
            m.main(output=captured)
            output = captured.getvalue()
            self.assertIn("boxart", output)
            self.assertIn("2", output)
            self.assertIn("screenshot", output)
            self.assertIn("3", output)
            self.assertIn("fanart", output)
            self.assertIn("1", output)


class TestMainApiErrorHandling(unittest.TestCase):
    """API errors must exit with a clear message, never a raw traceback."""

    def _run_sync_failing_with(self, error):
        """Run a sync where the syncer raises, capturing exit code and stderr."""
        with patch("sys.argv", ["main.py", "sync", "--platform", "nes", "--api-key", "testkey"]), \
             patch("main.MetadataDb") as MockDb, \
             patch("main.TheGamesDbClient") as MockClient, \
             patch("main.Syncer") as MockSyncer:
            mock_db_instance = MagicMock()
            MockDb.return_value.__enter__ = MagicMock(return_value=mock_db_instance)
            MockDb.return_value.__exit__ = MagicMock(return_value=False)
            MockClient.return_value = MagicMock()
            mock_syncer = MagicMock()
            mock_syncer.sync.side_effect = error
            MockSyncer.return_value = mock_syncer
            import main as m
            captured_err = StringIO()
            with patch("sys.stderr", captured_err):
                with self.assertRaises(SystemExit) as ctx:
                    m.main(output=StringIO())
            return ctx.exception.code, captured_err.getvalue()

    def test_sync_429_exits_with_friendly_allowance_message(self):
        from api_client import ApiError
        code, stderr = self._run_sync_failing_with(
            ApiError("API error 429: /v1/Games/Updates", status_code=429)
        )
        self.assertEqual(code, 1)
        self.assertIn("429", stderr)
        self.assertIn("allowance", stderr.lower())
        self.assertNotIn("Traceback", stderr)

    def test_sync_other_api_error_exits_with_error_message(self):
        from api_client import ApiError
        code, stderr = self._run_sync_failing_with(
            ApiError("API error 500: /v1/Games/Updates", status_code=500)
        )
        self.assertEqual(code, 1)
        self.assertIn("API error 500", stderr)
        self.assertNotIn("Traceback", stderr)


if __name__ == "__main__":
    unittest.main()
