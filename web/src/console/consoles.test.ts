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
});
