export function buildSaveStateKey({ name, size, hash }: { name: string; size: number; hash: string }) {
    return `rom:${name}:${size}:${hash}`;
}

export async function createRomSaveKey({ name, size, bytes }: { name: string; size: number; bytes: Uint8Array }) {
    const hash = await computeRomHash(bytes);
    return buildSaveStateKey({ name, size, hash });
}

export async function computeRomHash(bytes: Uint8Array | ArrayBuffer) {
    const data = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
    if (globalThis.crypto?.subtle?.digest) {
        const digest = await globalThis.crypto.subtle.digest("SHA-256", data as BufferSource);
        return bufferToHex(digest);
    }
    const { createHash } = await import("node:crypto");
    const hash = createHash("sha256").update(Buffer.from(data)).digest("hex");
    return hash;
}

export async function openSaveStateDb(name = "neser") {
    if (!globalThis.indexedDB) {
        throw new Error("IndexedDB not available");
    }
    return new Promise<IDBDatabase>((resolve, reject) => {
        const request = globalThis.indexedDB.open(name, 1);
        request.onerror = () => reject(request.error);
        request.onupgradeneeded = () => {
            const db = request.result;
            if (!db.objectStoreNames.contains("savestates")) {
                db.createObjectStore("savestates");
            }
        };
        request.onsuccess = () => resolve(request.result);
    });
}

export async function saveState(db: IDBDatabase, key: string, bytes: Uint8Array | ArrayBuffer) {
    const payload = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
    return new Promise<void>((resolve, reject) => {
        const tx = db.transaction("savestates", "readwrite");
        const store = tx.objectStore("savestates");
        const request = store.put(payload, key);
        request.onerror = () => reject(request.error);
        tx.oncomplete = () => resolve();
        tx.onerror = () => reject(tx.error || request.error);
        tx.onabort = () => reject(tx.error || request.error);
    });
}

export async function loadState(db: IDBDatabase, key: string) {
    return new Promise<Uint8Array | null>((resolve, reject) => {
        const tx = db.transaction("savestates", "readonly");
        const store = tx.objectStore("savestates");
        const request = store.get(key);
        request.onerror = () => reject(request.error);
        request.onsuccess = () => {
            const result = request.result;
            if (result) {
                resolve(result instanceof Uint8Array ? result : new Uint8Array(result));
            } else {
                resolve(null);
            }
        };
    });
}

export async function hasState(db: IDBDatabase, key: string) {
    return new Promise<boolean>((resolve, reject) => {
        const tx = db.transaction("savestates", "readonly");
        const store = tx.objectStore("savestates");
        const request = store.getKey(key);
        request.onerror = () => reject(request.error);
        request.onsuccess = () => resolve(request.result !== undefined);
    });
}

function bufferToHex(buffer: ArrayBuffer) {
    return Array.from(new Uint8Array(buffer), (byte) => byte.toString(16).padStart(2, "0")).join("");
}
