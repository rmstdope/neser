"""Sync orchestration: bulk and incremental sync from TheGamesDB."""
from datetime import datetime, timezone

from tqdm import tqdm

_BAR_FMT = "{desc}: {percentage:3.0f}%|{bar}| {n_fmt}/{total_fmt} [{elapsed}<{remaining}]"

class Syncer:
    """Coordinates fetching from TheGamesDbClient and persisting via MetadataDb."""

    def __init__(self, db, client, verbose: bool = False):
        self._db = db
        self._client = client
        self._verbose = verbose

    def _log(self, msg: str) -> None:
        if self._verbose:
            tqdm.write(msg)

    # ── public entry point ────────────────────────────────────────────────────

    def sync(self, platform_id: int, platform_info: dict, force_full: bool = False):
        """Dispatch to full_sync or incremental_sync based on existing sync_log."""
        log = self._db.get_sync_log(platform_id)
        if log is None or force_full:
            self.full_sync(platform_id=platform_id, platform_info=platform_info)
        else:
            self.incremental_sync(platform_id=platform_id, platform_info=platform_info)

    # ── full sync ─────────────────────────────────────────────────────────────

    def full_sync(self, platform_id: int, platform_info: dict):
        """Bulk-fetch all games and images for the given platform."""
        name = platform_info.get("name", f"Platform {platform_id}")
        label = platform_info.get("slug") or name
        self._db.upsert_platform(platform_info)
        self._fetch_reference_data(label)

        self._log(f"Downloading all games for {name}…")
        result = self._client.get_games_by_platform(platform_id)
        games = result["games"]
        base_url = result.get("base_url", {})
        boxart = result.get("boxart", {})

        if base_url:
            self._db.upsert_image_base_urls(base_url)

        game_ids = []
        pbar = tqdm(
            games,
            desc=f"  Games [{label}]",
            unit=" game",
            bar_format=_BAR_FMT,
            disable=not self._verbose,
        )
        for game in pbar:
            pbar.set_postfix_str(game.get("game_title", ""), refresh=False)
            self._db.upsert_game(game)
            game_ids.append(game["id"])

        # Store boxart images already returned in the platform response
        self._store_images(boxart)

        # Fetch all remaining image types via the Images endpoint
        if game_ids:
            self._log(f"Downloading images for {len(game_ids)} games…")
            images_result = self._client.get_games_images(game_ids)
            img_base_url = images_result.get("base_url", {})
            if img_base_url:
                self._db.upsert_image_base_urls(img_base_url)
            self._store_images_with_progress(
                images_result.get("images", {}),
                desc=f"  Images [{label}]",
            )

        now = datetime.now(timezone.utc).isoformat()
        self._db.update_sync_log(
            platform_id=platform_id,
            last_full_sync=now,
        )

    # ── incremental sync ──────────────────────────────────────────────────────

    def incremental_sync(self, platform_id: int, platform_info: dict):
        """Fetch only changed games since the last sync."""
        log = self._db.get_sync_log(platform_id)
        if log is None:
            raise RuntimeError(
                f"No prior full sync for platform {platform_id}. "
                "Run full_sync first."
            )

        last_edit_id = log.get("last_update_id") or 0
        updates_result = self._client.get_games_updates(last_edit_id=last_edit_id)
        updates = updates_result.get("updates") or []

        if not updates:
            # Nothing changed — still update the incremental timestamp
            now = datetime.now(timezone.utc).isoformat()
            self._db.update_sync_log(
                platform_id=platform_id,
                last_incremental_sync=now,
                last_update_id=last_edit_id,
            )
            return

        changed_ids = [u["game_id"] for u in updates]
        max_edit_id = max(u["edit_id"] for u in updates)

        games_result = self._client.get_games_by_id(changed_ids)
        games = games_result.get("games") or []
        platform_games = [g for g in games if g.get("platform") == platform_id]
        name = platform_info.get("name", f"Platform {platform_id}")
        label = platform_info.get("slug") or name
        pbar = tqdm(
            platform_games,
            desc=f"  Games [{label}]",
            unit=" game",
            bar_format=_BAR_FMT,
            disable=not self._verbose,
        )
        for game in pbar:
            pbar.set_postfix_str(game.get("game_title", ""), refresh=False)
            self._db.upsert_game(game)

        platform_game_ids = [g["id"] for g in platform_games]
        if platform_game_ids:
            self._log(f"Downloading images for {len(platform_game_ids)} updated games…")
            images_result = self._client.get_games_images(platform_game_ids)
            img_base_url = images_result.get("base_url", {})
            if img_base_url:
                self._db.upsert_image_base_urls(img_base_url)
            self._store_images_with_progress(
                images_result.get("images", {}),
                desc=f"  Images [{label}]",
            )

        now = datetime.now(timezone.utc).isoformat()
        self._db.update_sync_log(
            platform_id=platform_id,
            last_incremental_sync=now,
            last_update_id=max_edit_id,
        )

    # ── helpers ───────────────────────────────────────────────────────────────

    def _fetch_reference_data(self, label: str = ""):
        suffix = f" [{label}]" if label else ""
        steps = [
            ("genres",     self._client.get_genres),
            ("developers", self._client.get_developers),
            ("publishers", self._client.get_publishers),
            ("regions",    self._client.get_regions),
            ("countries",  self._client.get_countries),
        ]
        pbar = tqdm(
            steps,
            desc=f"  Reference data{suffix}",
            unit=" category",
            bar_format=_BAR_FMT,
            disable=not self._verbose,
        )
        for table, fetcher in pbar:
            pbar.set_postfix_str(table, refresh=False)
            data = fetcher()
            for entity in data.values():
                self._db.upsert_reference(table, entity)

    def _store_images(self, images_by_game: dict):
        """Persist image records without a progress bar (used for boxart from platform response)."""
        for game_id_key, img_list in images_by_game.items():
            try:
                game_id = int(game_id_key)
            except (ValueError, TypeError):
                continue
            for img in img_list:
                self._persist_image(game_id, img)

    def _store_images_with_progress(self, images_by_game: dict, desc: str = "  Images"):
        """Persist image records with a tqdm progress bar per game."""
        items = list(images_by_game.items())
        pbar = tqdm(
            items,
            desc=desc,
            unit=" game",
            bar_format=_BAR_FMT,
            disable=not self._verbose,
        )
        for game_id_key, img_list in pbar:
            try:
                game_id = int(game_id_key)
            except (ValueError, TypeError):
                continue
            for img in img_list:
                self._persist_image(game_id, img)

    def _persist_image(self, game_id: int, img: dict) -> None:
        self._db.upsert_image({
            "id": img["id"],
            "game_id": game_id,
            "type": img.get("type"),
            "side": img.get("side"),
            "filename": img.get("filename"),
            "resolution": img.get("resolution"),
        })
