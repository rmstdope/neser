import { describe, it, expect } from "vitest";
import {
    filterKeysForConsole,
    cycleFilterKey,
    filterOnConsoleSwitch,
    type FilterDef,
} from "./filters";

// ---------------------------------------------------------------------------
// Fixture: mirrors the real filter definitions from app.ts, extended with
// the new "gameboy" entry.
// ---------------------------------------------------------------------------
const filters: Record<string, FilterDef> = {
    stock: { name: "None", type: "single" },
    ntsc: { name: "NTSC", type: "ntsc" },
    crt: { name: "CRT", type: "single", params: {} },
    gameboy: { name: "Game Boy", type: "gb" },
};
const allKeys = Object.keys(filters); // stock, ntsc, crt, gameboy

// ===========================================================================
// filterKeysForConsole
// ===========================================================================
describe("filterKeysForConsole", () => {
    it("returns stock, ntsc, crt for NES (excludes gb)", () => {
        expect(filterKeysForConsole(allKeys, filters, "nes")).toEqual([
            "stock",
            "ntsc",
            "crt",
        ]);
    });

    it("returns stock and gameboy for GB", () => {
        expect(filterKeysForConsole(allKeys, filters, "gb")).toEqual([
            "stock",
            "gameboy",
        ]);
    });

    it("excludes CRT from GB (CRT is single type but not stock)", () => {
        const gbKeys = filterKeysForConsole(allKeys, filters, "gb");
        expect(gbKeys).not.toContain("crt");
    });

    it("excludes NTSC from GB", () => {
        const gbKeys = filterKeysForConsole(allKeys, filters, "gb");
        expect(gbKeys).not.toContain("ntsc");
    });

    it("excludes gameboy from NES", () => {
        const nesKeys = filterKeysForConsole(allKeys, filters, "nes");
        expect(nesKeys).not.toContain("gameboy");
    });

    it("returns only stock for GBA", () => {
        expect(filterKeysForConsole(allKeys, filters, "gba")).toEqual(["stock"]);
    });

    it("returns the NES looks for SNES: stock, ntsc, crt", () => {
        expect(filterKeysForConsole(allKeys, filters, "snes")).toEqual([
            "stock",
            "ntsc",
            "crt",
        ]);
    });
});

// ===========================================================================
// cycleFilterKey
// ===========================================================================
describe("cycleFilterKey", () => {
    // NES cycling: stock → ntsc → crt → stock → …
    it("NES: cycles from stock to ntsc", () => {
        expect(cycleFilterKey("stock", allKeys, filters, "nes")).toBe("ntsc");
    });

    it("NES: cycles from ntsc to crt", () => {
        expect(cycleFilterKey("ntsc", allKeys, filters, "nes")).toBe("crt");
    });

    it("NES: cycles from crt back to stock", () => {
        expect(cycleFilterKey("crt", allKeys, filters, "nes")).toBe("stock");
    });

    // GB cycling: stock → gameboy → stock → …
    it("GB: cycles from stock to gameboy", () => {
        expect(cycleFilterKey("stock", allKeys, filters, "gb")).toBe("gameboy");
    });

    it("GB: cycles from gameboy back to stock", () => {
        expect(cycleFilterKey("gameboy", allKeys, filters, "gb")).toBe("stock");
    });

    it("GBA: stays on stock", () => {
        expect(cycleFilterKey("stock", allKeys, filters, "gba")).toBe("stock");
    });

    // SNES cycling is the NES order: stock → ntsc → crt → stock → …
    it("SNES: cycles stock → ntsc → crt → stock", () => {
        expect(cycleFilterKey("stock", allKeys, filters, "snes")).toBe("ntsc");
        expect(cycleFilterKey("ntsc", allKeys, filters, "snes")).toBe("crt");
        expect(cycleFilterKey("crt", allKeys, filters, "snes")).toBe("stock");
    });

    it("returns current filter when no filters available", () => {
        const empty: Record<string, FilterDef> = {};
        expect(cycleFilterKey("stock", [], empty, "nes")).toBe("stock");
    });

    it("wraps to first when current filter is unknown", () => {
        // If current filter isn't in the available list, indexOf returns -1,
        // (-1 + 1) % N = 0, so it picks the first available filter.
        expect(cycleFilterKey("unknown", allKeys, filters, "nes")).toBe("stock");
    });
});

// ===========================================================================
// filterOnConsoleSwitch
// ===========================================================================
describe("filterOnConsoleSwitch", () => {
    it("keeps stock when switching from NES to GB", () => {
        expect(filterOnConsoleSwitch("stock", allKeys, filters, "gb")).toBe(
            "stock",
        );
    });

    it("falls back to gameboy when switching from NES ntsc to GB", () => {
        expect(filterOnConsoleSwitch("ntsc", allKeys, filters, "gb")).toBe(
            "gameboy",
        );
    });

    it("falls back to gameboy when switching from NES crt to GB", () => {
        expect(filterOnConsoleSwitch("crt", allKeys, filters, "gb")).toBe(
            "gameboy",
        );
    });

    it("keeps stock when switching from GB to NES", () => {
        expect(filterOnConsoleSwitch("stock", allKeys, filters, "nes")).toBe(
            "stock",
        );
    });

    it("falls back to ntsc when switching from GB gameboy to NES", () => {
        expect(filterOnConsoleSwitch("gameboy", allKeys, filters, "nes")).toBe(
            "ntsc",
        );
    });

    it("keeps gameboy when switching from GB to GB (no-op)", () => {
        expect(filterOnConsoleSwitch("gameboy", allKeys, filters, "gb")).toBe(
            "gameboy",
        );
    });

    it("falls back to stock when switching to GBA", () => {
        expect(filterOnConsoleSwitch("ntsc", allKeys, filters, "gba")).toBe(
            "stock",
        );
        expect(filterOnConsoleSwitch("gameboy", allKeys, filters, "gba")).toBe(
            "stock",
        );
    });

    it("keeps NTSC and CRT when switching from NES to SNES", () => {
        expect(filterOnConsoleSwitch("ntsc", allKeys, filters, "snes")).toBe("ntsc");
        expect(filterOnConsoleSwitch("crt", allKeys, filters, "snes")).toBe("crt");
    });

    it("falls back to stock when switching from GB gameboy to SNES", () => {
        expect(filterOnConsoleSwitch("gameboy", allKeys, filters, "snes")).toBe(
            "stock",
        );
    });

    it("keeps an SNES look when switching from SNES to NES", () => {
        expect(filterOnConsoleSwitch("crt", allKeys, filters, "nes")).toBe("crt");
    });

    // Nobody has chosen a look and no game has loaded yet: each console starts on its own
    // default, whatever the page's initial filter is.
    it("untouched: each console starts on its default", () => {
        expect(filterOnConsoleSwitch("ntsc", allKeys, filters, "snes", true)).toBe("stock");
        expect(filterOnConsoleSwitch("ntsc", allKeys, filters, "nes", true)).toBe("ntsc");
        expect(filterOnConsoleSwitch("ntsc", allKeys, filters, "gb", true)).toBe("gameboy");
        expect(filterOnConsoleSwitch("ntsc", allKeys, filters, "gba", true)).toBe("stock");
    });

    it("touched: SNES keeps a chosen look even on the first game", () => {
        expect(filterOnConsoleSwitch("crt", allKeys, filters, "snes", false)).toBe("crt");
    });
});
