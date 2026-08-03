"""HTTP client for TheGamesDB REST API v1."""

import time
from urllib.parse import urlencode

import requests
from tqdm import tqdm

from scripts.metadata_scraper import JsonDict

_BASE_URL = "https://api.thegamesdb.net"

_ALL_FIELDS = ",".join(
    [
        "players",
        "publishers",
        "genres",
        "overview",
        "last_updated",
        "rating",
        "platform",
        "coop",
        "youtube",
        "os",
        "processor",
        "ram",
        "hdd",
        "video",
        "sound",
        "alternates",
    ]
)


class ApiError(Exception):
    """Raised when the API returns a non-200 status code.

    ``status_code`` holds the HTTP status code, or ``None`` when the failure
    was not an HTTP error status (e.g. an invalid JSON body).
    """

    def __init__(self, message: str, status_code: int | None = None) -> None:
        super().__init__(message)
        self.status_code = status_code


class TheGamesDbClient:
    """Thin, rate-limited client for TheGamesDB REST API."""

    def __init__(self, api_key: str, request_delay: float = 0.2, batch_size: int = 100, verbose: bool = False) -> None:
        if not api_key:
            raise ValueError("api_key must not be empty")
        self._api_key = api_key
        self._request_delay = request_delay
        self._batch_size = batch_size
        self._verbose = verbose
        self._last_request_time: float = 0.0

    # ── internal helpers ──────────────────────────────────────────────────────

    def _get(self, path: str, params: JsonDict) -> JsonDict:
        if self._last_request_time:
            time.sleep(self._request_delay)

        params = dict(params)
        params["apikey"] = self._api_key
        url = f"{_BASE_URL}{path}?{urlencode(params)}"
        resp = requests.get(url, timeout=30)
        self._last_request_time = time.time()

        if resp.status_code != 200:
            raise ApiError(f"API error {resp.status_code}: {path}", status_code=resp.status_code)
        try:
            payload: JsonDict = resp.json()
            return payload
        except Exception as exc:
            raise ApiError(f"Invalid JSON from {path}: {exc}") from exc

    def _paginate(self, path: str, params: JsonDict) -> JsonDict:
        """Fetch all pages and merge the games lists."""
        all_games: list[JsonDict] = []
        base_url: JsonDict = {}
        boxart: JsonDict = {}
        page = 1
        pbar = tqdm(
            desc="  Fetching pages",
            unit=" page",
            bar_format="{desc}: page {n_fmt} | {postfix} [{elapsed}]",
            disable=not self._verbose,
        )
        while True:
            p = dict(params)
            p["page"] = page
            data = self._get(path, p)
            games = data.get("data", {}).get("games") or []
            all_games.extend(games)
            pbar.update(1)
            pbar.set_postfix_str(f"{len(all_games)} games fetched", refresh=True)

            include = data.get("include", {})
            if "boxart" in include:
                bu = include["boxart"].get("base_url", {})
                if bu:
                    base_url = bu
                boxart_data = include["boxart"].get("data", {})
                for gid, imgs in boxart_data.items():
                    boxart.setdefault(gid, []).extend(imgs)

            pages = data.get("pages", {})
            if not pages.get("next"):
                break
            page += 1
        pbar.close()
        return {"games": all_games, "base_url": base_url, "boxart": boxart}

    def _batch_ids(self, ids: list[int]) -> list[list[int]]:
        return [ids[i : i + self._batch_size] for i in range(0, len(ids), self._batch_size)]

    # ── public API ────────────────────────────────────────────────────────────

    def get_games_by_platform(self, platform_id: int) -> JsonDict:
        """Fetch all games for a platform (all pages), with all fields and boxart."""
        params = {
            "id": platform_id,
            "fields": _ALL_FIELDS,
            "include": "boxart",
        }
        return self._paginate("/v1/Games/ByPlatformID", params)

    def get_games_by_id(self, game_ids: list[int]) -> JsonDict:
        """Fetch game(s) by id; batches automatically."""
        all_games: list[JsonDict] = []
        for batch in self._batch_ids(game_ids):
            params = {
                "id": ",".join(str(i) for i in batch),
                "fields": _ALL_FIELDS,
            }
            data = self._get("/v1/Games/ByGameID", params)
            games = data.get("data", {}).get("games") or []
            if isinstance(games, dict):
                all_games.extend(games.values())
            else:
                all_games.extend(games)
        return {"games": all_games}

    def get_games_images(self, game_ids: list[int]) -> JsonDict:
        """Fetch all image types for the given game ids; batches and paginates automatically."""
        all_images: dict[str, list[JsonDict]] = {}
        combined_base_url: JsonDict = {}
        for batch in self._batch_ids(game_ids):
            page = 1
            while True:
                params = {"games_id": ",".join(str(i) for i in batch), "page": page}
                data = self._get("/v1/Games/Images", params)
                payload = data.get("data", {})
                bu = payload.get("base_url") or {}
                if bu:
                    combined_base_url = bu
                for game_id_key, imgs in (payload.get("images") or {}).items():
                    all_images.setdefault(game_id_key, []).extend(imgs)
                if not data.get("pages", {}).get("next"):
                    break
                page += 1
        return {"images": all_images, "base_url": combined_base_url}

    def get_games_updates(self, last_edit_id: int) -> JsonDict:
        """Fetch game update log since last_edit_id."""
        data = self._get("/v1/Games/Updates", {"last_edit_id": last_edit_id})
        updates: JsonDict = data.get("data", {})
        return updates

    def get_genres(self) -> JsonDict:
        data = self._get("/v1/Genres", {})
        result: JsonDict = data.get("data", {}).get("genres", {})
        return result

    def get_developers(self) -> JsonDict:
        data = self._get("/v1/Developers", {})
        result: JsonDict = data.get("data", {}).get("developers", {})
        return result

    def get_publishers(self) -> JsonDict:
        data = self._get("/v1/Publishers", {})
        result: JsonDict = data.get("data", {}).get("publishers", {})
        return result

    def get_regions(self) -> JsonDict:
        data = self._get("/v1/Regions", {})
        result: JsonDict = data.get("data", {}).get("regions", {})
        return result

    def get_countries(self) -> JsonDict:
        data = self._get("/v1/Countries", {})
        result: JsonDict = data.get("data", {}).get("countries", {})
        return result

    def get_api_limit(self) -> JsonDict:
        # Unlike every other endpoint, /v1/API/Limit has no "data" wrapper:
        # remaining_monthly_allowance, extra_allowance and
        # allowance_refresh_timer sit at the top level of the response.
        return self._get("/v1/API/Limit", {})
