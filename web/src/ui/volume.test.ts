import { describe, expect, it } from "vitest";

import { DEFAULT_VOLUME, parseStoredVolume, volumeIconState, volumeToGain } from "./volume";

describe("volume", () => {
    it("Given the slider's percent, When turned into a gain, Then it follows a squared curve so the low end is usable", () => {
        expect(volumeToGain(0)).toBe(0);
        expect(volumeToGain(50)).toBeCloseTo(0.25);
        expect(volumeToGain(100)).toBe(1);
    });

    it("Given an out-of-range percent, When turned into a gain, Then it is clamped to 0..1", () => {
        expect(volumeToGain(-20)).toBe(0);
        expect(volumeToGain(140)).toBe(1);
    });

    it("Given nothing stored, When the page starts, Then the volume is full", () => {
        expect(DEFAULT_VOLUME).toBe(100);
        expect(parseStoredVolume(null)).toBe(100);
    });

    it("Given a stored value, When it is read back, Then a whole percent 0..100 is restored and anything else falls back to the default", () => {
        expect(parseStoredVolume("35")).toBe(35);
        expect(parseStoredVolume("0")).toBe(0);
        expect(parseStoredVolume("250")).toBe(100);
        expect(parseStoredVolume("loud")).toBe(100);
        expect(parseStoredVolume("12.7")).toBe(13);
    });

    it("Given mute and the volume, When choosing the speaker icon, Then muted or zero shows the crossed speaker", () => {
        expect(volumeIconState(false, 60)).toBe("on");
        expect(volumeIconState(true, 60)).toBe("off");
        expect(volumeIconState(false, 0)).toBe("off");
    });
});
