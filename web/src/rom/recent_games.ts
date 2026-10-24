// The games last played in this browser, for the library's "Continue playing" row. Each keeps
// its bytes (so it plays again without the file), when it was last played, and a small picture
// of its last frame. Kept in its own IndexedDB database, like the SNES firmware.

// Version 2 added the bytes store; a version 1 database (one store) is upgraded in place.
const DB_VERSION = 2;
/** What a card shows, one record per game, small enough to list on every visit. */
const GAMES = "games";
/** Each game's ROM bytes, keyed by its name, read only when its card is played. */
const BYTES = "bytes";

/** How many games the row remembers; the least recently played beyond this are forgotten. */
export const RECENT_GAMES_LIMIT = 8;

export type RecentGame = {
    /**
     * The name it was loaded under: a file name, or a bundled ROM's path. It is the key, so two
     * different files of the same name share one card, the later one replacing the earlier.
     */
    name: string;
    /** The system badge it ran as (NES, GB, GBC, GBA, SNES). */
    system: string;
    /** Chosen from a file (rather than the bundled list), which autorun recording needs. */
    fromFile: boolean;
    /** When it was last started, in milliseconds since the epoch. */
    playedAt: number;
    /** A PNG data URL of its last frame, once one was taken. */
    thumbnail: string | null;
};

export type RecentGamesStore = {
    /** Every remembered game, the latest played first, without its bytes. */
    list(): Promise<RecentGame[]>;
    /** A remembered game's ROM bytes; null once it is forgotten. */
    bytesOf(name: string): Promise<Uint8Array | null>;
    /** Remembers a game as just played, keeping a picture it already had, and forgets the oldest beyond the limit. */
    remember(game: RecentGame & { bytes: Uint8Array }): Promise<void>;
    /** Sets a remembered game's picture; a game no longer remembered is left alone. */
    setThumbnail(name: string, thumbnail: string): Promise<void>;
    /** Forgets every game. */
    clear(): Promise<void>;
};

function openDb(name: string): Promise<IDBDatabase> {
    if (!globalThis.indexedDB) {
        return Promise.reject(new Error("IndexedDB not available"));
    }
    return new Promise((resolve, reject) => {
        const request = globalThis.indexedDB.open(name, DB_VERSION);
        request.onerror = () => reject(request.error);
        request.onupgradeneeded = () => {
            const db = request.result;
            if (!db.objectStoreNames.contains(GAMES)) db.createObjectStore(GAMES, { keyPath: "name" });
            if (!db.objectStoreNames.contains(BYTES)) db.createObjectStore(BYTES);
        };
        request.onsuccess = () => resolve(request.result);
    });
}

/** Runs `action` in one transaction over both stores and resolves once it has committed. */
function transact<T>(
    db: IDBDatabase,
    mode: IDBTransactionMode,
    action: (games: IDBObjectStore, bytes: IDBObjectStore, done: (value: T) => void) => void
) {
    return new Promise<T>((resolve, reject) => {
        const tx = db.transaction([GAMES, BYTES], mode);
        let result: T;
        action(tx.objectStore(GAMES), tx.objectStore(BYTES), (value) => {
            result = value;
        });
        tx.oncomplete = () => resolve(result);
        tx.onerror = () => reject(tx.error);
        tx.onabort = () => reject(tx.error);
    });
}

function allGames(games: IDBObjectStore, done: (list: RecentGame[]) => void) {
    const request = games.getAll();
    request.onsuccess = () => done((request.result as RecentGame[]).sort((a, b) => b.playedAt - a.playedAt));
}

/** The recent games store in the database `name` (default `neser-recent`). */
export function createRecentGamesStore(name = "neser-recent"): RecentGamesStore {
    let dbPromise: Promise<IDBDatabase> | null = null;
    const db = () => (dbPromise ??= openDb(name));
    return {
        async list() {
            return transact<RecentGame[]>(await db(), "readonly", (games, _bytes, done) => allGames(games, done));
        },
        async bytesOf(gameName) {
            const value = await transact<unknown>(await db(), "readonly", (_games, bytes, done) => {
                const request = bytes.get(gameName);
                request.onsuccess = () => done(request.result);
            });
            if (!value) return null;
            return value instanceof Uint8Array ? value : new Uint8Array(value as ArrayBuffer);
        },
        async remember({ bytes: romBytes, ...game }) {
            await transact<void>(await db(), "readwrite", (games, bytes) => {
                const existing = games.get(game.name);
                existing.onsuccess = () => {
                    const thumbnail = game.thumbnail ?? (existing.result as RecentGame | undefined)?.thumbnail ?? null;
                    games.put({ ...game, thumbnail });
                    bytes.put(romBytes, game.name);
                    allGames(games, (list) => {
                        const kept = new Set(
                            [game, ...list.filter((g) => g.name !== game.name)].slice(0, RECENT_GAMES_LIMIT).map((g) => g.name)
                        );
                        for (const old of list) {
                            if (kept.has(old.name)) continue;
                            games.delete(old.name);
                            bytes.delete(old.name);
                        }
                    });
                };
            });
        },
        async setThumbnail(gameName, thumbnail) {
            await transact<void>(await db(), "readwrite", (games) => {
                const existing = games.get(gameName);
                existing.onsuccess = () => {
                    const game = existing.result as RecentGame | undefined;
                    if (game) games.put({ ...game, thumbnail });
                };
            });
        },
        async clear() {
            await transact<void>(await db(), "readwrite", (games, bytes) => {
                games.clear();
                bytes.clear();
            });
        },
    };
}

/**
 * Where a picture waits, in `localStorage`, until its database write has committed. A picture
 * is taken as the page is hidden or left, and Chromium aborts every database write the page has
 * not finished when it goes, so on a slow machine the picture would be lost with the page
 * (nr-39ib). `localStorage` is written at once, and the next page finishes the write.
 */
export const PENDING_THUMBNAIL_KEY = "neser.recentPendingThumbnail";

type PendingStorage = Pick<Storage, "getItem" | "setItem" | "removeItem">;

/** Sets a game's picture so that it outlives the page: noted at once, then written to `store`. */
export async function keepThumbnail(store: RecentGamesStore, storage: PendingStorage | null, name: string, thumbnail: string): Promise<void> {
    const note = JSON.stringify({ name, thumbnail });
    try {
        storage?.setItem(PENDING_THUMBNAIL_KEY, note);
    } catch {
        // Too full or not allowed: the database write is then the only copy.
    }
    await store.setThumbnail(name, thumbnail);
    try {
        // A later picture's note stays until its own write commits.
        if (storage?.getItem(PENDING_THUMBNAIL_KEY) === note) storage.removeItem(PENDING_THUMBNAIL_KEY);
    } catch {
        // Nothing to tidy where storage cannot be read.
    }
}

/** Finishes writing a picture the last page noted but could not write before it went. */
export async function applyPendingThumbnail(store: RecentGamesStore, storage: PendingStorage | null): Promise<void> {
    let note: string | null;
    try {
        note = storage?.getItem(PENDING_THUMBNAIL_KEY) ?? null;
    } catch {
        return;
    }
    if (!note) return;
    let pending: { name?: unknown; thumbnail?: unknown } | null = null;
    try {
        pending = JSON.parse(note) as { name?: unknown; thumbnail?: unknown } | null;
    } catch {
        // Not a note this page wrote; dropped below.
    }
    if (typeof pending?.name === "string" && typeof pending.thumbnail === "string") {
        await keepThumbnail(store, storage, pending.name, pending.thumbnail);
        return;
    }
    try {
        storage?.removeItem(PENDING_THUMBNAIL_KEY);
    } catch {
        // Left for the next page.
    }
}

/** When a game was last played, in words: Today, Yesterday, "3 days ago" within a week, "Last week" within two, else the date. */
export function playedWhen(now: Date, then: Date): string {
    const startOfDay = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
    const days = Math.round((startOfDay(now) - startOfDay(then)) / 86_400_000);
    if (days <= 0) return "Today";
    if (days === 1) return "Yesterday";
    if (days < 7) return `${days} days ago`;
    if (days < 14) return "Last week";
    return new Date(then.getFullYear(), then.getMonth(), then.getDate()).toLocaleDateString();
}

/** Draws one card per recent game into `row`; pressing a card plays that game. */
export function renderRecentGames(row: HTMLElement, games: RecentGame[], now: Date, onPick: (game: RecentGame) => void): void {
    row.replaceChildren(
        ...games.map((game) => {
            const file = game.name.slice(game.name.lastIndexOf("/") + 1);
            const dot = file.lastIndexOf(".");
            const title = dot > 0 ? file.slice(0, dot) : file;
            const card = document.createElement("button");
            card.type = "button";
            card.className = "recent-card";
            card.setAttribute("aria-label", `Play ${title} again`);
            const picture = document.createElement("span");
            picture.className = "recent-card-picture";
            if (game.thumbnail) {
                const img = document.createElement("img");
                img.src = game.thumbnail;
                img.alt = "";
                picture.append(img);
            }
            const badge = document.createElement("span");
            badge.className = "sys-badge";
            badge.dataset.system = game.system;
            badge.textContent = game.system;
            picture.append(badge);
            const name = document.createElement("span");
            name.className = "recent-card-name";
            name.textContent = title;
            const when = document.createElement("span");
            when.className = "recent-card-when";
            when.textContent = playedWhen(now, new Date(game.playedAt));
            card.append(picture, name, when);
            card.addEventListener("click", () => onPick(game));
            return card;
        })
    );
}
