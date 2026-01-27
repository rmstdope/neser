import "fake-indexeddb/auto";
import assert from "node:assert/strict";
import test from "node:test";
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

test("computeRomHash returns stable SHA-256 hex", async () => {
    const bytes = new Uint8Array([0, 1, 2]);
    const hash = await computeRomHash(bytes);
    assert.equal(hash, "ae4b3280e56e2faf83f414a6e3dabe9d5fbe18976544c05fed121accb85b53fc");
});

test("buildSaveStateKey includes name, size, and hash", () => {
    const key = buildSaveStateKey({
        name: "Test.nes",
        size: 3,
        hash: "abc123"
    });
    assert.equal(key, "rom:Test.nes:3:abc123");
});

test("createRomSaveKey hashes and formats key", async () => {
    const bytes = new Uint8Array([0, 1, 2]);
    const key = await createRomSaveKey({
        name: "Test.nes",
        size: 3,
        bytes
    });
    assert.equal(key, "rom:Test.nes:3:ae4b3280e56e2faf83f414a6e3dabe9d5fbe18976544c05fed121accb85b53fc");
});

test("saveState and loadState roundtrip bytes", async () => {
    const db = await openSaveStateDb(createDbName());
    const key = "rom:Test.nes:3:abc123";
    const payload = new Uint8Array([9, 8, 7, 6]);

    await saveState(db, key, payload);
    const loaded = await loadState(db, key);

    assert.ok(loaded);
    assert.deepEqual(Array.from(loaded), Array.from(payload));
});

test("hasState reports presence", async () => {
    const db = await openSaveStateDb(createDbName());
    const key = "rom:Test.nes:3:abc123";

    const before = await hasState(db, key);
    assert.equal(before, false);

    await saveState(db, key, new Uint8Array([1]));

    const after = await hasState(db, key);
    assert.equal(after, true);
});
