import { describe, expect, it } from "vitest";
import { paletteButtonVisible, cyclePaletteTo, listPalettes } from "./palette_button";

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

    it("stays in its place while paused (the app disables it, since no new frame would show the change)", () => {
        expect(paletteButtonVisible({ kind: "nes", label: "Palette: Default", running: true, paused: true })).toBe(true);
    });

    it("is hidden once the game has been stopped", () => {
        expect(paletteButtonVisible({ kind: "nes", label: "Palette: Default", running: false, paused: false })).toBe(false);
    });
});

describe("listing and choosing a palette by cycling", () => {
    const cycler = (names: string[], at = 0) => {
        let i = at;
        return { label: () => names[i], cycle: () => void (i = (i + 1) % names.length), at: () => i };
    };

    it("Given the core can only cycle, When its palettes are listed, Then all are named in order from the current one, and it ends where it began", () => {
        const source = cycler(["Default", "NesDev", "Smooth"], 1);
        expect(listPalettes(source)).toEqual(["NesDev", "Smooth", "Default"]);
        expect(source.at()).toBe(1);
    });

    it("Given a palette is picked, When cycled to, Then the core lands on it", () => {
        const source = cycler(["Default", "NesDev", "Smooth"]);
        expect(cyclePaletteTo(source, "Smooth")).toBe(true);
        expect(source.label()).toBe("Smooth");
    });

    it("Given a name the core does not have, When cycled to, Then it stops where it began and says so", () => {
        const source = cycler(["Default", "NesDev"]);
        expect(cyclePaletteTo(source, "Nope")).toBe(false);
        expect(source.label()).toBe("Default");
    });
});
