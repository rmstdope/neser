import { expect, it } from "vitest";
import { buildFontString, sampleWaveY } from "./sine_scroller.js";

it("sampleWaveY returns base offset", () => {
    const y = sampleWaveY({
        x: 0,
        timeSeconds: 0,
        baseY: 10,
        amplitude: 5,
        frequency: 0
    });
    expect(y).toBe(10);
});

it("sampleWaveY follows sine phase", () => {
    const y = sampleWaveY({
        x: 0,
        timeSeconds: Math.PI / 2,
        baseY: 10,
        amplitude: 5,
        frequency: 0
    });
    expect(Math.abs(y - 15) < 1e-6).toBeTruthy();
});

it("buildFontString formats size", () => {
    expect(buildFontString(24, "'Courier New', monospace")).toBe("bold 24px 'Courier New', monospace");
});
