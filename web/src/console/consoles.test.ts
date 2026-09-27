import { describe, expect, it } from "vitest";
import { CONSOLE_KINDS, CONSOLES, consoleKindForExtension } from "./consoles";

describe("the console table", () => {
    it("has exactly one row per console kind", () => {
        expect(Object.keys(CONSOLES).sort()).toEqual([...CONSOLE_KINDS].sort());
    });

    it("never gives one file extension to two consoles", () => {
        const all = CONSOLE_KINDS.flatMap((kind) => CONSOLES[kind].extensions);
        expect(new Set(all).size).toBe(all.length);
    });

    it("writes extensions in lower case without a dot", () => {
        for (const kind of CONSOLE_KINDS) {
            for (const ext of CONSOLES[kind].extensions) {
                expect(ext).toBe(ext.toLowerCase());
                expect(ext.startsWith(".")).toBe(false);
            }
        }
    });

    it("finds the console for each of its extensions, ignoring case", () => {
        for (const kind of CONSOLE_KINDS) {
            for (const ext of CONSOLES[kind].extensions) {
                expect(consoleKindForExtension(ext)).toBe(kind);
                expect(consoleKindForExtension(ext.toUpperCase())).toBe(kind);
            }
        }
    });

    it("finds no console for an unknown or empty extension", () => {
        expect(consoleKindForExtension("zip")).toBeNull();
        expect(consoleKindForExtension("")).toBeNull();
    });

    it("shows help for at least one player on every console", () => {
        for (const kind of CONSOLE_KINDS) {
            expect(CONSOLES[kind].playerKeyBindings.length).toBeGreaterThan(0);
        }
    });
});
