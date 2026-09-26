import { expect, it } from "vitest";
import "fake-indexeddb/auto";
import { createSnesFirmwareStore } from "./snes_firmware_store";

function dbName() {
    return `neser-firmware-test-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

it("holds nothing until a firmware is stored", async () => {
    const store = createSnesFirmwareStore(dbName());
    expect(await store.load("dsp1")).toBeNull();
    expect(await store.storedKeys()).toEqual([]);
});

it("keeps a stored firmware across store instances (a reload) until it is forgotten", async () => {
    const name = dbName();
    const bytes = new Uint8Array(8192).fill(0x1b);
    await createSnesFirmwareStore(name).store("dsp1", bytes);

    const reloaded = createSnesFirmwareStore(name);
    expect(await reloaded.load("dsp1")).toEqual(bytes);

    await reloaded.forget("dsp1");
    expect(await createSnesFirmwareStore(name).load("dsp1")).toBeNull();
});

it("keeps each chip's file separately, and forgetting one keeps the other", async () => {
    const store = createSnesFirmwareStore(dbName());
    await store.store("dsp2", new Uint8Array(8192).fill(2));
    await store.store("dsp1", new Uint8Array(8192).fill(1));
    expect((await store.storedKeys()).sort()).toEqual(["dsp1", "dsp2"]);
    expect((await store.load("dsp1"))![0]).toBe(1);
    expect((await store.load("dsp2"))![0]).toBe(2);

    await store.forget("dsp2");
    expect(await store.load("dsp2")).toBeNull();
    expect((await store.load("dsp1"))![0]).toBe(1);
    expect(await store.storedKeys()).toEqual(["dsp1"]);
});

it("replacing overwrites only that chip's firmware", async () => {
    const store = createSnesFirmwareStore(dbName());
    await store.store("dsp1", new Uint8Array(8192).fill(1));
    await store.store("dsp1", new Uint8Array(8192).fill(3));
    expect((await store.load("dsp1"))![0]).toBe(3);
});

it("reads a DSP-1 file stored before per-chip storage (same database and key)", async () => {
    // nr-auv stored the DSP-1 firmware in `neser-firmware`/`firmware` under the key "dsp1".
    const name = dbName();
    await new Promise<void>((resolve, reject) => {
        const request = indexedDB.open(name, 1);
        request.onupgradeneeded = () => request.result.createObjectStore("firmware");
        request.onerror = () => reject(request.error);
        request.onsuccess = () => {
            const tx = request.result.transaction("firmware", "readwrite");
            tx.objectStore("firmware").put(new Uint8Array(8192).fill(0x1b), "dsp1");
            tx.oncomplete = () => {
                request.result.close();
                resolve();
            };
        };
    });
    expect((await createSnesFirmwareStore(name).load("dsp1"))![0]).toBe(0x1b);
});
