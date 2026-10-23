import { describe, expect, it } from "vitest";

import { DEFAULT_VOLUME, VOLUME_STORAGE_KEY, parseStoredVolume, readStoredVolume, volumeIconState, volumeToGain, writeStoredVolume } from "./volume";

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
        expect(parseStoredVolume("")).toBe(100);
        expect(parseStoredVolume("  ")).toBe(100);
    });

    it("Given the browser's storage, When the volume is written and read back, Then it survives", () => {
        const store = new Map<string, string>();
        const storage = { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => void store.set(k, v) };
        writeStoredVolume(storage, 35);
        expect(store.get(VOLUME_STORAGE_KEY)).toBe("35");
        expect(readStoredVolume(storage)).toBe(35);
    });

    it("Given storage that refuses or is missing, When the volume is read or written, Then the default is used and nothing throws", () => {
        const refusing = { getItem: () => { throw new Error("denied"); }, setItem: () => { throw new Error("denied"); } };
        expect(readStoredVolume(refusing)).toBe(100);
        expect(() => writeStoredVolume(refusing, 20)).not.toThrow();
        expect(readStoredVolume(null)).toBe(100);
        expect(() => writeStoredVolume(null, 20)).not.toThrow();
    });

    it("Given mute and the volume, When choosing the speaker icon, Then muted or zero shows the crossed speaker", () => {
        expect(volumeIconState(false, 60)).toBe("on");
        expect(volumeIconState(true, 60)).toBe("off");
        expect(volumeIconState(false, 0)).toBe("off");
    });
});
