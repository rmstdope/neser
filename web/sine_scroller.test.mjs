import test from "node:test";
import assert from "node:assert/strict";
import { buildFontString, sampleWaveY } from "./sine_scroller.js";

test("sampleWaveY returns base offset", () => {
    const y = sampleWaveY({
        x: 0,
        timeSeconds: 0,
        baseY: 10,
        amplitude: 5,
        frequency: 0
    });
    assert.equal(y, 10);
});

test("sampleWaveY follows sine phase", () => {
    const y = sampleWaveY({
        x: 0,
        timeSeconds: Math.PI / 2,
        baseY: 10,
        amplitude: 5,
        frequency: 0
    });
    assert.ok(Math.abs(y - 15) < 1e-6);
});

test("buildFontString formats size", () => {
    assert.equal(buildFontString(24, "'Courier New', monospace"), "bold 24px 'Courier New', monospace");
});
