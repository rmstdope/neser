import { describe, expect, it } from "vitest";
import { normalizeGbSample, normalizeGbaSample, normalizeNesSample } from "./audio_normalizer";

describe("normalizeGbSample", () => {
    it("passes through a sample already in range", () => {
        expect(normalizeGbSample(0.5)).toBe(0.5);
        expect(normalizeGbSample(-0.5)).toBe(-0.5);
        expect(normalizeGbSample(0.0)).toBe(0.0);
    });

    it("clamps positive overflow to 1.0", () => {
        expect(normalizeGbSample(1.5)).toBe(1.0);
        expect(normalizeGbSample(1.0)).toBe(1.0);
    });

    it("clamps negative overflow to -1.0 (bipolar — not 0)", () => {
        expect(normalizeGbSample(-1.5)).toBe(-1.0);
        expect(normalizeGbSample(-1.0)).toBe(-1.0);
    });

    it("preserves negative samples in range (half-cycle not discarded)", () => {
        expect(normalizeGbSample(-0.75)).toBe(-0.75);
        expect(normalizeGbSample(-0.01)).toBeCloseTo(-0.01);
    });
});

describe("normalizeGbaSample", () => {
    it("matches native output gain before clamping direct-sound samples", () => {
        expect(normalizeGbaSample(1.0)).toBeCloseTo(0.75);
        expect(normalizeGbaSample(-1.0)).toBeCloseTo(-0.75);
    });
});

describe("normalizeNesSample", () => {
    const NES_APU_MAX = 1.177;

    it("normalises a mid-range sample", () => {
        expect(normalizeNesSample(NES_APU_MAX / 2, NES_APU_MAX)).toBeCloseTo(0.5);
    });

    it("clamps to 1.0 for samples at or above nesApuMax", () => {
        expect(normalizeNesSample(NES_APU_MAX, NES_APU_MAX)).toBeCloseTo(1.0);
        expect(normalizeNesSample(NES_APU_MAX * 2, NES_APU_MAX)).toBe(1.0);
    });

    it("clamps to 0.0 for zero or negative samples", () => {
        expect(normalizeNesSample(0.0, NES_APU_MAX)).toBe(0.0);
        expect(normalizeNesSample(-0.5, NES_APU_MAX)).toBe(0.0);
    });
});
