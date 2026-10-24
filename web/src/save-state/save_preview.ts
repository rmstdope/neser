// What the Load button shows on hover: a picture of the saved moment and when it was saved. One
// preview per game, beside the save state itself, in its own IndexedDB database so the save-state
// database's schema is untouched.

import { playedWhen } from "../rom/recent_games";

const DB_VERSION = 1;
const STORE = "previews";

export type SavePreview = {
    /** A PNG data URL of the frame the state was saved on; null when none could be taken. */
    thumbnail: string | null;
    /** When it was saved, in milliseconds since the epoch. */
    savedAt: number;
};

export type SavePreviewStore = {
    get(game: string): Promise<SavePreview | null>;
    put(game: string, preview: SavePreview): Promise<void>;
};

function openDb(name: string): Promise<IDBDatabase> {
    if (!globalThis.indexedDB) {
        return Promise.reject(new Error("IndexedDB not available"));
    }
    return new Promise((resolve, reject) => {
        const request = globalThis.indexedDB.open(name, DB_VERSION);
        request.onerror = () => reject(request.error);
        request.onupgradeneeded = () => {
            if (!request.result.objectStoreNames.contains(STORE)) request.result.createObjectStore(STORE);
        };
        request.onsuccess = () => resolve(request.result);
    });
}

function run<T>(db: IDBDatabase, mode: IDBTransactionMode, action: (store: IDBObjectStore) => IDBRequest<T>) {
    return new Promise<T>((resolve, reject) => {
        const tx = db.transaction(STORE, mode);
        const request = action(tx.objectStore(STORE));
        tx.oncomplete = () => resolve(request.result);
        tx.onerror = () => reject(tx.error);
        tx.onabort = () => reject(tx.error);
    });
}

/** The save preview store in the database `name` (default `neser-save-previews`). */
export function createSavePreviewStore(name = "neser-save-previews"): SavePreviewStore {
    let dbPromise: Promise<IDBDatabase> | null = null;
    const db = () => (dbPromise ??= openDb(name));
    return {
        async get(game) {
            const preview = await run(await db(), "readonly", (s) => s.get(game));
            return (preview as SavePreview | undefined) ?? null;
        },
        async put(game, preview) {
            await run(await db(), "readwrite", (s) => s.put(preview, game));
        },
    };
}

/** How long ago a state was saved: just now, minutes, hours, then days as `playedWhen` says them. */
export function savedAgo(now: number, savedAt: number): string {
    const minutes = Math.floor((now - savedAt) / 60_000);
    if (minutes < 1) return "Saved just now";
    if (minutes < 60) return `Saved ${minutes} minute${minutes === 1 ? "" : "s"} ago`;
    const day = playedWhen(new Date(now), new Date(savedAt));
    if (day === "Today") {
        const hours = Math.floor(minutes / 60);
        return `Saved ${hours} hour${hours === 1 ? "" : "s"} ago`;
    }
    if (day === "Yesterday") return "Saved yesterday";
    if (day === "Last week") return "Saved last week";
    return day.endsWith("days ago") ? `Saved ${day}` : `Saved on ${day}`;
}

/** The save's day and time: "Today 14:02", "Yesterday 09:05", or further back the date and time. */
export function savedAtLine(now: Date, savedAt: number): string {
    const at = new Date(savedAt);
    const time = `${String(at.getHours()).padStart(2, "0")}:${String(at.getMinutes()).padStart(2, "0")}`;
    const day = playedWhen(now, at);
    return `${day === "Today" || day === "Yesterday" ? day : at.toLocaleDateString()} ${time}`;
}
