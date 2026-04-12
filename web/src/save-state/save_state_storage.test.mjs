import { expect, it } from "vitest";
import "fake-indexeddb/auto";
import {
    buildSaveStateKey,
    createRomSaveKey,
    computeRomHash,
    hasState,
    openSaveStateDb,
    saveState,
    loadState
} from "./save_state_storage.js";

function createDbName() {
    return `neser-test-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

it("computeRomHash returns stable SHA-256 hex", async () => {
    const bytes = new Uint8Array([0, 1, 2]);
    const hash = await computeRomHash(bytes);
    expect(hash).toBe("ae4b3280e56e2faf83f414a6e3dabe9d5fbe18976544c05fed121accb85b53fc");
});

it("buildSaveStateKey includes name, size, and hash", () => {
    const key = buildSaveStateKey({
        name: "Test.nes",
        size: 3,
        hash: "abc123"
    });
    expect(key).toBe("rom:Test.nes:3:abc123");
});

it("createRomSaveKey hashes and formats key", async () => {
    const bytes = new Uint8Array([0, 1, 2]);
    const key = await createRomSaveKey({
        name: "Test.nes",
        size: 3,
        bytes
    });
    expect(key).toBe("rom:Test.nes:3:ae4b3280e56e2faf83f414a6e3dabe9d5fbe18976544c05fed121accb85b53fc");
});

it("saveState and loadState roundtrip bytes", async () => {
    const db = await openSaveStateDb(createDbName());
    const key = "rom:Test.nes:3:abc123";
    const payload = new Uint8Array([9, 8, 7, 6]);

    await saveState(db, key, payload);
    const loaded = await loadState(db, key);

    expect(loaded).toBeTruthy();
    expect(Array.from(loaded)).toEqual(Array.from(payload));
});

it("hasState reports presence", async () => {
    const db = await openSaveStateDb(createDbName());
    const key = "rom:Test.nes:3:abc123";

    const before = await hasState(db, key);
    expect(before).toBe(false);

    await saveState(db, key, new Uint8Array([1]));

    const after = await hasState(db, key);
    expect(after).toBe(true);
});
