// The games last played in this browser, for the library's "Continue playing" row. Each keeps
// its bytes (so it plays again without the file), when it was last played, and a small picture
// of its last frame. Kept in its own IndexedDB database, like the SNES firmware.

const DB_VERSION = 1;
const STORE = "games";

/** How many games the row remembers; the least recently played beyond this are forgotten. */
export const RECENT_GAMES_LIMIT = 8;

export type RecentGame = {
    /** The name it was loaded under: a file name, or a bundled ROM's path. */
    name: string;
    /** The system badge it ran as (NES, GB, GBC, GBA, SNES). */
    system: string;
    bytes: Uint8Array;
    /** Chosen from a file (rather than the bundled list), which autorun recording needs. */
    fromFile: boolean;
    /** When it was last started, in milliseconds since the epoch. */
    playedAt: number;
    /** A PNG data URL of its last frame, once one was taken. */
    thumbnail: string | null;
};

export type RecentGamesStore = {
    /** Every remembered game, the latest played first. */
    list(): Promise<RecentGame[]>;
    /** Remembers a game as just played, keeping a picture it already had, and forgets the oldest beyond the limit. */
    remember(game: RecentGame): Promise<void>;
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
            if (!request.result.objectStoreNames.contains(STORE)) {
                request.result.createObjectStore(STORE, { keyPath: "name" });
            }
        };
        request.onsuccess = () => resolve(request.result);
    });
}

/** Runs `action` in one transaction and resolves once it has committed. */
function transact<T>(db: IDBDatabase, mode: IDBTransactionMode, action: (store: IDBObjectStore, done: (value: T) => void) => void) {
    return new Promise<T>((resolve, reject) => {
        const tx = db.transaction(STORE, mode);
        let result: T;
        action(tx.objectStore(STORE), (value) => {
            result = value;
        });
        tx.oncomplete = () => resolve(result);
        tx.onerror = () => reject(tx.error);
        tx.onabort = () => reject(tx.error);
    });
}

function allGames(store: IDBObjectStore, done: (games: RecentGame[]) => void) {
    const request = store.getAll();
    request.onsuccess = () => {
        const games = (request.result as RecentGame[]).map((game) => ({
            ...game,
            bytes: game.bytes instanceof Uint8Array ? game.bytes : new Uint8Array(game.bytes as ArrayBuffer),
        }));
        done(games.sort((a, b) => b.playedAt - a.playedAt));
    };
}

/** The recent games store in the database `name` (default `neser-recent`). */
export function createRecentGamesStore(name = "neser-recent"): RecentGamesStore {
    let dbPromise: Promise<IDBDatabase> | null = null;
    const db = () => (dbPromise ??= openDb(name));
    return {
        async list() {
            return transact<RecentGame[]>(await db(), "readonly", (store, done) => allGames(store, done));
        },
        async remember(game) {
            await transact<void>(await db(), "readwrite", (store) => {
                const existing = store.get(game.name);
                existing.onsuccess = () => {
                    const thumbnail = game.thumbnail ?? (existing.result as RecentGame | undefined)?.thumbnail ?? null;
                    store.put({ ...game, thumbnail });
                    allGames(store, (games) => {
                        const kept = new Set(
                            [game, ...games.filter((g) => g.name !== game.name)].slice(0, RECENT_GAMES_LIMIT).map((g) => g.name)
                        );
                        for (const old of games) if (!kept.has(old.name)) store.delete(old.name);
                    });
                };
            });
        },
        async setThumbnail(gameName, thumbnail) {
            await transact<void>(await db(), "readwrite", (store) => {
                const existing = store.get(gameName);
                existing.onsuccess = () => {
                    const game = existing.result as RecentGame | undefined;
                    if (game) store.put({ ...game, thumbnail });
                };
            });
        },
        async clear() {
            await transact<void>(await db(), "readwrite", (store) => {
                store.clear();
            });
        },
    };
}

/** When a game was last played, in words: Today, Yesterday, "3 days ago" within a week, else the date. */
export function playedWhen(now: Date, then: Date): string {
    const startOfDay = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
    const days = Math.round((startOfDay(now) - startOfDay(then)) / 86_400_000);
    if (days <= 0) return "Today";
    if (days === 1) return "Yesterday";
    if (days < 7) return `${days} days ago`;
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
