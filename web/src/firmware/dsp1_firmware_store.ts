// The DSP-1 firmware the player chose, kept in this browser across reloads (nr-auv).
//
// It lives in its own IndexedDB database so the save-state database's schema is untouched.

const DB_VERSION = 1;
const STORE = "firmware";
const DSP1_KEY = "dsp1";

export type FirmwareStore = {
    load(): Promise<Uint8Array | null>;
    store(bytes: Uint8Array): Promise<void>;
    forget(): Promise<void>;
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

/** The DSP-1 firmware store in the database `name` (default `neser-firmware`). */
export function createDsp1FirmwareStore(name = "neser-firmware"): FirmwareStore {
    let dbPromise: Promise<IDBDatabase> | null = null;
    const db = () => (dbPromise ??= openDb(name));
    return {
        async load() {
            const value = await run(await db(), "readonly", (s) => s.get(DSP1_KEY));
            if (!value) return null;
            return value instanceof Uint8Array ? value : new Uint8Array(value as ArrayBuffer);
        },
        async store(bytes) {
            await run(await db(), "readwrite", (s) => s.put(bytes, DSP1_KEY));
        },
        async forget() {
            await run(await db(), "readwrite", (s) => s.delete(DSP1_KEY));
        }
    };
}
