import test from "node:test";
import assert from "node:assert/strict";
import { computePlaybackRate } from "./audio_resampler.js";

test("computePlaybackRate returns 1.0 at target latency", () => {
    const rate = computePlaybackRate({
        latencySeconds: 0.1,
        targetLatencySeconds: 0.1
    });
    assert.equal(rate, 1.0);
});

test("computePlaybackRate clamps to max adjust", () => {
    const lowRate = computePlaybackRate({
        latencySeconds: 0.0,
        targetLatencySeconds: 0.1,
        maxAdjust: 0.005,
        gain: 1.0
    });
    assert.equal(lowRate, 0.995);

    const highRate = computePlaybackRate({
        latencySeconds: 0.2,
        targetLatencySeconds: 0.1,
        maxAdjust: 0.005,
        gain: 1.0
    });
    assert.equal(highRate, 1.005);
});

test("computePlaybackRate scales within range", () => {
    const rate = computePlaybackRate({
        latencySeconds: 0.12,
        targetLatencySeconds: 0.1,
        maxAdjust: 0.005,
        gain: 0.1
    });
    assert.equal(rate, 1.002);
});
