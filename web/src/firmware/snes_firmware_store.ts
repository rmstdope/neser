// The SNES coprocessor firmware the player chose, one file per chip, kept in this browser across
// reloads (nr-auv, nr-608).
//
// It lives in its own IndexedDB database so the save-state database's schema is untouched. Each
// chip's file is stored under its chip key ("dsp1", "dsp2"); "dsp1" is the key nr-auv used, so a
// DSP-1 file stored before per-chip storage is still found.

const DB_VERSION = 1;
const STORE = "firmware";

export type FirmwareStore = {
    load(key: string): Promise<Uint8Array | null>;
    store(key: string, bytes: Uint8Array): Promise<void>;
    forget(key: string): Promise<void>;
    /** The keys of every chip with a stored file, in no particular order. */
    storedKeys(): Promise<string[]>;
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
                request.result.createObjectStore(STORE);
            }
        };
        request.onsuccess = () => resolve(request.result);
    });
}

function run<T>(db: IDBDatabase, mode: IDBTransactionMode, action: (store: IDBObjectStore) => IDBRequest<T>) {
    return new Promise<T>((resolve, reject) => {
        const tx = db.transaction(STORE, mode);
        const request = action(tx.objectStore(STORE));
        let result: T;
        request.onsuccess = () => {
            result = request.result;
        };
        tx.oncomplete = () => resolve(result);
        tx.onerror = () => reject(tx.error || request.error);
        tx.onabort = () => reject(tx.error || request.error);
    });
}

/** The SNES firmware store in the database `name` (default `neser-firmware`). */
export function createSnesFirmwareStore(name = "neser-firmware"): FirmwareStore {
    let dbPromise: Promise<IDBDatabase> | null = null;
    const db = () => (dbPromise ??= openDb(name));
    return {
        async load(key) {
            const value = await run(await db(), "readonly", (s) => s.get(key));
            if (!value) return null;
            return value instanceof Uint8Array ? value : new Uint8Array(value as ArrayBuffer);
        },
        async store(key, bytes) {
            await run(await db(), "readwrite", (s) => s.put(bytes, key));
        },
        async forget(key) {
            await run(await db(), "readwrite", (s) => s.delete(key));
        },
        async storedKeys() {
            const keys = await run(await db(), "readonly", (s) => s.getAllKeys());
            return keys.map(String);
        }
    };
}
