"""Tests for the CLI entrypoint in main.py."""
import sys
import unittest
from io import StringIO
from unittest.mock import MagicMock, patch


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

    def test_sync_all_platforms_syncs_nes_gb_gbc_gba(self):
        _, _, syncer, _ = self._run(["sync", "--platform", "all", "--api-key", "testkey"])
        platform_ids = [c[1]["platform_id"] for c in syncer.sync.call_args_list]
        self.assertIn(7, platform_ids)   # NES
        self.assertIn(4, platform_ids)   # GB
        self.assertIn(41, platform_ids)  # GBC
        self.assertIn(5, platform_ids)   # GBA


if __name__ == "__main__":
    unittest.main()
