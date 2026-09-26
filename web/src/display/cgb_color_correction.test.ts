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
    it("is shown while a colour Game Boy game is running or paused", () => {
        expect(cgbColorButtonVisible({ kind: "gb", isColor: true, active: true })).toBe(true);
    });

    it("is hidden when no game is loaded", () => {
        expect(cgbColorButtonVisible({ kind: null, isColor: false, active: false })).toBe(false);
    });

    it("is hidden for a Game Boy game shown in black and white", () => {
        expect(cgbColorButtonVisible({ kind: "gb", isColor: false, active: true })).toBe(false);
    });

    it("is hidden for NES, SNES and Game Boy Advance games", () => {
        for (const kind of ["nes", "snes", "gba"] as const) {
            expect(cgbColorButtonVisible({ kind, isColor: true, active: true })).toBe(false);
        }
    });

    it("is hidden once the colour game has been stopped", () => {
        expect(cgbColorButtonVisible({ kind: "gb", isColor: true, active: false })).toBe(false);
    });
});
