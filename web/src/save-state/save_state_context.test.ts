import { expect, it } from "vitest";
import { createSaveStateContext } from "./save_state_context";

it("createSaveStateContext returns controller when nes and rom metadata exist", async () => {
    const calls: any[] = [];
    const nes = { id: "nes" };
    const romMetadata = { name: "Test.nes", size: 3, bytes: new Uint8Array([1, 2, 3]) };
    const controller = { save: async () => true, load: async () => true };

    const result = await createSaveStateContext({
        nes,
        romMetadata,
        openDb: async () => ({ name: "db" }),
        createRomSaveKey: async () => "rom:Test.nes:3:hash",
        createSaveStateController: ({ nes: passedNes, db, key }: any) => {
            calls.push({ passedNes, db, key });
            return controller;
        },
        saveStateFn: async () => undefined,
        loadStateFn: async () => null,
        setStatus: () => undefined
    } as any);

    expect(result).toBe(controller);
    expect(calls[0]).toEqual({
        passedNes: nes,
        db: { name: "db" },
        key: "rom:Test.nes:3:hash"
    });
});

it("createSaveStateContext returns null when missing nes or rom metadata", async () => {
    const result1 = await createSaveStateContext({
        nes: null,
        romMetadata: { name: "Test.nes", size: 3, bytes: new Uint8Array([1]) },
        openDb: async () => ({}),
        createRomSaveKey: async () => "key",
        createSaveStateController: () => ({}),
        saveStateFn: async () => undefined,
        loadStateFn: async () => null,
        setStatus: () => undefined
    } as any);
    const result2 = await createSaveStateContext({
        nes: { id: "nes" },
        romMetadata: null,
        openDb: async () => ({}),
        createRomSaveKey: async () => "key",
        createSaveStateController: () => ({}),
        saveStateFn: async () => undefined,
        loadStateFn: async () => null,
        setStatus: () => undefined
    } as any);

    expect(result1).toBe(null);
    expect(result2).toBe(null);
});
