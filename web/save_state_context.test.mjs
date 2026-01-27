import assert from "node:assert/strict";
import test from "node:test";
import { createSaveStateContext } from "./save_state_context.js";

test("createSaveStateContext returns controller when nes and rom metadata exist", async () => {
    const calls = [];
    const nes = { id: "nes" };
    const romMetadata = { name: "Test.nes", size: 3, bytes: new Uint8Array([1, 2, 3]) };
    const controller = { save: async () => true, load: async () => true };

    const result = await createSaveStateContext({
        nes,
        romMetadata,
        openDb: async () => ({ name: "db" }),
        createRomSaveKey: async () => "rom:Test.nes:3:hash",
        createSaveStateController: ({ nes: passedNes, db, key }) => {
            calls.push({ passedNes, db, key });
            return controller;
        },
        saveStateFn: async () => undefined,
        loadStateFn: async () => null,
        setStatus: () => undefined
    });

    assert.equal(result, controller);
    assert.deepEqual(calls[0], {
        passedNes: nes,
        db: { name: "db" },
        key: "rom:Test.nes:3:hash"
    });
});

test("createSaveStateContext returns null when missing nes or rom metadata", async () => {
    const result1 = await createSaveStateContext({
        nes: null,
        romMetadata: { name: "Test.nes", size: 3, bytes: new Uint8Array([1]) },
        openDb: async () => ({}),
        createRomSaveKey: async () => "key",
        createSaveStateController: () => ({}),
        saveStateFn: async () => undefined,
        loadStateFn: async () => null,
        setStatus: () => undefined
    });
    const result2 = await createSaveStateContext({
        nes: { id: "nes" },
        romMetadata: null,
        openDb: async () => ({}),
        createRomSaveKey: async () => "key",
        createSaveStateController: () => ({}),
        saveStateFn: async () => undefined,
        loadStateFn: async () => null,
        setStatus: () => undefined
    });

    assert.equal(result1, null);
    assert.equal(result2, null);
});
