import { describe, expect, it } from "vitest";
import { cgbColorButtonLabel, cgbColorButtonVisible } from "./cgb_color_correction";

describe("cgbColorButtonLabel", () => {
    it("names the raw state when correction is off", () => {
        expect(cgbColorButtonLabel(false)).toBe("Colors: Raw");
    });

    it("names the GBC screen state when correction is on", () => {
        expect(cgbColorButtonLabel(true)).toBe("Colors: GBC screen");
    });
});

describe("cgbColorButtonVisible", () => {
    it("is shown while a colour Game Boy game is running", () => {
        expect(cgbColorButtonVisible({ kind: "gb", isColor: true, running: true, paused: false })).toBe(true);
    });

    it("is hidden when no game is loaded", () => {
        expect(cgbColorButtonVisible({ kind: null, isColor: false, running: false, paused: false })).toBe(false);
    });

    it("is hidden for a Game Boy game shown in black and white", () => {
        expect(cgbColorButtonVisible({ kind: "gb", isColor: false, running: true, paused: false })).toBe(false);
    });

    it("is hidden for NES and SNES games", () => {
        for (const kind of ["nes", "snes"] as const) {
            expect(cgbColorButtonVisible({ kind, isColor: true, running: true, paused: false })).toBe(false);
        }
    });

    it("is shown while a Game Boy Advance game is running", () => {
        expect(cgbColorButtonVisible({ kind: "gba", isColor: false, running: true, paused: false })).toBe(true);
    });

    it("is hidden while a Game Boy Advance game is paused or stopped", () => {
        expect(cgbColorButtonVisible({ kind: "gba", isColor: false, running: true, paused: true })).toBe(false);
        expect(cgbColorButtonVisible({ kind: "gba", isColor: false, running: false, paused: false })).toBe(false);
    });

    it("is hidden while the colour game is paused, since no new frame would show the change", () => {
        expect(cgbColorButtonVisible({ kind: "gb", isColor: true, running: true, paused: true })).toBe(false);
    });

    it("is hidden once the colour game has been stopped", () => {
        expect(cgbColorButtonVisible({ kind: "gb", isColor: true, running: false, paused: false })).toBe(false);
    });
});
