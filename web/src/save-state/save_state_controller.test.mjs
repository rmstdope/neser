import { expect, it } from "vitest";
import { createSaveStateController } from "./save_state_controller.js";

it("save handler stores bytes and sets status", async () => {
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

    expect(result).toBe(true);
    expect(calls[0]).toEqual({
        db: { name: "db" },
        key: "rom:Test.nes:3:abc",
        bytes: [1, 2, 3]
    });
    expect(calls[1]).toEqual({ status: "State saved", isError: false });
});

it("load handler loads bytes into nes and sets status", async () => {
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

    expect(result).toBe(true);
    expect(calls[0]).toEqual({ loaded: [9, 8, 7] });
    expect(calls[1]).toEqual({ status: "State loaded", isError: false });
});

it("load handler reports missing state", async () => {
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

    expect(result).toBe(false);
    expect(calls[0]).toEqual({ status: "No save state found", isError: true });
});

it("save handler logs errors", async () => {
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
    expect(result).toBe(false);
    expect(calls.length).toBe(1);
});

it("load handler logs errors", async () => {
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
    expect(result).toBe(false);
    expect(calls.length).toBe(1);
});
