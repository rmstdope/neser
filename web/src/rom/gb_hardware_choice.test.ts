import { describe, expect, it } from "vitest";
import {
    GB_HARDWARE_STORAGE_KEY,
    gbHardwareNoteVisible,
    readGbHardwareChoice,
    writeGbHardwareChoice
} from "./gb_hardware_choice";

function memoryStorage(initial: Record<string, string> = {}) {
    const items = new Map(Object.entries(initial));
    return {
        items,
        getItem: (key: string) => items.get(key) ?? null,
        setItem: (key: string, value: string) => void items.set(key, value)
    };
}

const throwing = {
    getItem: (): string | null => {
        throw new Error("blocked");
    },
    setItem: (): void => {
        throw new Error("blocked");
    }
};

describe("readGbHardwareChoice", () => {
    it("is Game Boy on a first visit", () => {
        expect(readGbHardwareChoice(memoryStorage())).toBe("dmg");
    });

    it("is Game Boy when the browser keeps nothing", () => {
        expect(readGbHardwareChoice(null)).toBe("dmg");
        expect(readGbHardwareChoice(throwing)).toBe("dmg");
    });

    it("is the remembered choice", () => {
        expect(readGbHardwareChoice(memoryStorage({ [GB_HARDWARE_STORAGE_KEY]: "cgb" }))).toBe("cgb");
        expect(readGbHardwareChoice(memoryStorage({ [GB_HARDWARE_STORAGE_KEY]: "dmg" }))).toBe("dmg");
    });

    it("reads anything unknown as Game Boy", () => {
        expect(readGbHardwareChoice(memoryStorage({ [GB_HARDWARE_STORAGE_KEY]: "gba" }))).toBe("dmg");
    });
});

describe("writeGbHardwareChoice", () => {
    it("remembers the choice", () => {
        const storage = memoryStorage();
        writeGbHardwareChoice(storage, "cgb");
        expect(storage.items.get(GB_HARDWARE_STORAGE_KEY)).toBe("cgb");
        expect(readGbHardwareChoice(storage)).toBe("cgb");
    });

    it("stays silent when the browser keeps nothing", () => {
        expect(() => writeGbHardwareChoice(null, "cgb")).not.toThrow();
        expect(() => writeGbHardwareChoice(throwing, "cgb")).not.toThrow();
    });
});

describe("gbHardwareNoteVisible", () => {
    const originalOnGameBoy = { kind: "gb" as const, running: true, isOriginalGame: true, runsOnColor: false };

    it("shows while an original Game Boy game runs on the other console", () => {
        expect(gbHardwareNoteVisible({ ...originalOnGameBoy, choice: "cgb" })).toBe(true);
        expect(gbHardwareNoteVisible({ ...originalOnGameBoy, runsOnColor: true, choice: "dmg" })).toBe(true);
    });

    it("is hidden while it runs on the chosen console", () => {
        expect(gbHardwareNoteVisible({ ...originalOnGameBoy, choice: "dmg" })).toBe(false);
        expect(gbHardwareNoteVisible({ ...originalOnGameBoy, runsOnColor: true, choice: "cgb" })).toBe(false);
    });

    it("is hidden with no game", () => {
        expect(gbHardwareNoteVisible({ kind: null, running: false, isOriginalGame: false, runsOnColor: false, choice: "cgb" })).toBe(false);
        expect(gbHardwareNoteVisible({ ...originalOnGameBoy, running: false, choice: "cgb" })).toBe(false);
    });

    it("is hidden for a Game Boy Color game", () => {
        expect(gbHardwareNoteVisible({ kind: "gb", running: true, isOriginalGame: false, runsOnColor: true, choice: "dmg" })).toBe(false);
    });

    it("is hidden for other consoles' games", () => {
        for (const kind of ["nes", "snes", "gba"] as const) {
            expect(gbHardwareNoteVisible({ ...originalOnGameBoy, kind, choice: "cgb" })).toBe(false);
        }
    });
});
