import { describe, expect, it } from "vitest";
import { CONSOLE_KINDS, CONSOLES } from "./consoles";

describe("the console table", () => {
    it("gives GBA games their own looks and starts them on None", () => {
        expect(CONSOLES.gba.filterFamily).toBe("gba");
        expect(CONSOLES.gba.defaultFilter).toBe("stock");
    });

    it("has exactly one row per console kind", () => {
        expect(Object.keys(CONSOLES).sort()).toEqual([...CONSOLE_KINDS].sort());
    });

    it("carries no ROM extensions: those come from the wasm table", () => {
        for (const kind of CONSOLE_KINDS) {
            expect("extensions" in CONSOLES[kind]).toBe(false);
        }
    });

    it("shows help for at least one player on every console", () => {
        for (const kind of CONSOLE_KINDS) {
            expect(CONSOLES[kind].playerKeyBindings.length).toBeGreaterThan(0);
        }
    });

    it("names only the Game Boy Advance's own buttons, on the keys the web binds them to (F B, G A, V L, B R)", () => {
        expect(CONSOLES.gba.playerKeyBindings).toEqual(["W/A/S/D: D-Pad\nG: A\nF: B\nV: L\nB: R\n4: Select\n5: Start"]);
    });
});
