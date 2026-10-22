import { expect, it } from "vitest";
import { buildFontString, sampleWaveY, SCROLLER_LETTER_COLORS } from "./sine_scroller";

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

it("the scroller letters run from ice white to steel blue with a steel glow (Steel Glow theme)", () => {
    expect(SCROLLER_LETTER_COLORS).toEqual({ top: "#eef6ff", middle: "#b9dcff", bottom: "#7fb0e3", glow: "#7fb0e3" });
});
