import { expect, it } from "vitest";
import "fake-indexeddb/auto";
import { createDsp1FirmwareStore } from "./dsp1_firmware_store";

function dbName() {
    return `neser-firmware-test-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

it("holds nothing until a firmware is stored", async () => {
    expect(await createDsp1FirmwareStore(dbName()).load()).toBeNull();
});

it("keeps a stored firmware across store instances (a reload) until it is forgotten", async () => {
    const name = dbName();
    const bytes = new Uint8Array(8192).fill(0x1b);
    await createDsp1FirmwareStore(name).store(bytes);

    const reloaded = createDsp1FirmwareStore(name);
    expect(await reloaded.load()).toEqual(bytes);

    await reloaded.forget();
    expect(await createDsp1FirmwareStore(name).load()).toBeNull();
});

it("replacing overwrites the stored firmware", async () => {
    const store = createDsp1FirmwareStore(dbName());
    await store.store(new Uint8Array(8192).fill(1));
    await store.store(new Uint8Array(8192).fill(2));
    expect((await store.load())![0]).toBe(2);
});
