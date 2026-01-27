import assert from "node:assert/strict";
import test from "node:test";
import { createSaveStateController } from "./save_state_controller.js";

test("save handler stores bytes and sets status", async () => {
    const calls = [];
    const controller = createSaveStateController({
        nes: {
            save_state_bytes() {
                return new Uint8Array([1, 2, 3]);
            }
        },
        db: { name: "db" },
        key: "rom:Test.nes:3:abc",
        saveStateFn: async (db, key, bytes) => {
            calls.push({ db, key, bytes: Array.from(bytes) });
        },
        loadStateFn: async () => null,
        setStatus: (message, isError = false) => {
            calls.push({ status: message, isError });
        }
    });

    const result = await controller.save();

    assert.equal(result, true);
    assert.deepEqual(calls[0], {
        db: { name: "db" },
        key: "rom:Test.nes:3:abc",
        bytes: [1, 2, 3]
    });
    assert.deepEqual(calls[1], { status: "State saved", isError: false });
});

test("load handler loads bytes into nes and sets status", async () => {
    const calls = [];
    const controller = createSaveStateController({
        nes: {
            load_state_bytes(bytes) {
                calls.push({ loaded: Array.from(bytes) });
                return undefined;
            }
        },
        db: { name: "db" },
        key: "rom:Test.nes:3:abc",
        saveStateFn: async () => undefined,
        loadStateFn: async () => new Uint8Array([9, 8, 7]),
        setStatus: (message, isError = false) => {
            calls.push({ status: message, isError });
        }
    });

    const result = await controller.load();

    assert.equal(result, true);
    assert.deepEqual(calls[0], { loaded: [9, 8, 7] });
    assert.deepEqual(calls[1], { status: "State loaded", isError: false });
});

test("load handler reports missing state", async () => {
    const calls = [];
    const controller = createSaveStateController({
        nes: {
            load_state_bytes() {
                throw new Error("should not be called");
            }
        },
        db: { name: "db" },
        key: "rom:Test.nes:3:abc",
        saveStateFn: async () => undefined,
        loadStateFn: async () => null,
        setStatus: (message, isError = false) => {
            calls.push({ status: message, isError });
        }
    });

    const result = await controller.load();

    assert.equal(result, false);
    assert.deepEqual(calls[0], { status: "No save state found", isError: true });
});

test("save handler logs errors", async () => {
    const calls = [];
    const originalError = console.error;
    console.error = (...args) => calls.push(args);

    const controller = createSaveStateController({
        nes: {
            save_state_bytes() {
                throw new Error("boom");
            }
        },
        db: { name: "db" },
        key: "rom:Test.nes:3:abc",
        saveStateFn: async () => undefined,
        loadStateFn: async () => null,
        setStatus: () => undefined
    });

    const result = await controller.save();

    console.error = originalError;
    assert.equal(result, false);
    assert.equal(calls.length, 1);
});

test("load handler logs errors", async () => {
    const calls = [];
    const originalError = console.error;
    console.error = (...args) => calls.push(args);

    const controller = createSaveStateController({
        nes: {
            load_state_bytes() {
                throw new Error("boom");
            }
        },
        db: { name: "db" },
        key: "rom:Test.nes:3:abc",
        saveStateFn: async () => undefined,
        loadStateFn: async () => new Uint8Array([1]),
        setStatus: () => undefined
    });

    const result = await controller.load();

    console.error = originalError;
    assert.equal(result, false);
    assert.equal(calls.length, 1);
});
