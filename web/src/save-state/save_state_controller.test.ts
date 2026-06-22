import { expect, it } from "vitest";
import { createSaveStateController } from "./save_state_controller";

it("save handler stores bytes and sets status", async () => {
    const calls: any[] = [];
    const controller = createSaveStateController({
        runtime: {
            save_state_bytes() {
                return new Uint8Array([1, 2, 3]);
            }
        },
        db: { name: "db" },
        key: "rom:Test.nes:3:abc",
        saveStateFn: async (db: any, key: any, bytes: any) => {
            calls.push({ db, key, bytes: Array.from(bytes) });
        },
        loadStateFn: async () => null,
        setStatus: (message: string, isError = false) => {
            calls.push({ status: message, isError });
        }
    } as any);

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
    const calls: any[] = [];
    const controller = createSaveStateController({
        runtime: {
            load_state_bytes(bytes: any) {
                calls.push({ loaded: Array.from(bytes) });
                return undefined;
            }
        },
        db: { name: "db" },
        key: "rom:Test.nes:3:abc",
        saveStateFn: async () => undefined,
        loadStateFn: async () => new Uint8Array([9, 8, 7]),
        setStatus: (message: string, isError = false) => {
            calls.push({ status: message, isError });
        }
    } as any);

    const result = await controller.load();

    expect(result).toBe(true);
    expect(calls[0]).toEqual({ loaded: [9, 8, 7] });
    expect(calls[1]).toEqual({ status: "State loaded", isError: false });
});

it("load handler reports missing state", async () => {
    const calls: any[] = [];
    const controller = createSaveStateController({
        runtime: {
            load_state_bytes() {
                throw new Error("should not be called");
            }
        },
        db: { name: "db" },
        key: "rom:Test.nes:3:abc",
        saveStateFn: async () => undefined,
        loadStateFn: async () => null,
        setStatus: (message: string, isError = false) => {
            calls.push({ status: message, isError });
        }
    } as any);

    const result = await controller.load();

    expect(result).toBe(false);
    expect(calls[0]).toEqual({ status: "No save state found", isError: true });
});

it("save handler logs errors", async () => {
    const calls: any[] = [];
    const originalError = console.error;
    console.error = (...args: any[]) => calls.push(args);

    const controller = createSaveStateController({
        runtime: {
            save_state_bytes() {
                throw new Error("boom");
            }
        },
        db: { name: "db" },
        key: "rom:Test.nes:3:abc",
        saveStateFn: async () => undefined,
        loadStateFn: async () => null,
        setStatus: () => undefined
    } as any);

    const result = await controller.save();

    console.error = originalError;
    expect(result).toBe(false);
    expect(calls.length).toBe(1);
});

it("load handler logs errors", async () => {
    const calls: any[] = [];
    const originalError = console.error;
    console.error = (...args: any[]) => calls.push(args);

    const controller = createSaveStateController({
        runtime: {
            load_state_bytes() {
                throw new Error("boom");
            }
        },
        db: { name: "db" },
        key: "rom:Test.nes:3:abc",
        saveStateFn: async () => undefined,
        loadStateFn: async () => new Uint8Array([1]),
        setStatus: () => undefined
    } as any);

    const result = await controller.load();

    console.error = originalError;
    expect(result).toBe(false);
    expect(calls.length).toBe(1);
});
