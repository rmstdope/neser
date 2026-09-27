import { describe, expect, it } from "vitest";
import { paletteButtonVisible } from "./palette_button";

const running = { running: true, paused: false };

describe("paletteButtonVisible", () => {
    it("is shown while an NES game is running", () => {
        expect(paletteButtonVisible({ kind: "nes", label: "Palette: Default", ...running })).toBe(true);
    });

    it("is shown while an original Game Boy game is running", () => {
        expect(paletteButtonVisible({ kind: "gb", label: "Palette: Grey", ...running })).toBe(true);
    });

    it("is hidden when no game is loaded", () => {
        expect(paletteButtonVisible({ kind: null, label: "", running: false, paused: false })).toBe(false);
    });

    it("is hidden for a Game Boy Color game, where F8 does nothing", () => {
        expect(paletteButtonVisible({ kind: "gb", label: "", ...running })).toBe(false);
    });

    it("is hidden for Super Nintendo and Game Boy Advance games", () => {
        for (const kind of ["snes", "gba"] as const) {
            expect(paletteButtonVisible({ kind, label: "Palette: Default", ...running })).toBe(false);
        }
    });

    it("is hidden while paused, since no new frame would show the change", () => {
        expect(paletteButtonVisible({ kind: "nes", label: "Palette: Default", running: true, paused: true })).toBe(false);
    });

    it("is hidden once the game has been stopped", () => {
        expect(paletteButtonVisible({ kind: "nes", label: "Palette: Default", running: false, paused: false })).toBe(false);
    });
});
