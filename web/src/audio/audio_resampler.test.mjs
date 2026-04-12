import { expect, it } from "vitest";
import { computePlaybackRate } from "./audio_resampler.js";

it("computePlaybackRate returns 1.0 at target latency", () => {
    const rate = computePlaybackRate({
        latencySeconds: 0.1,
        targetLatencySeconds: 0.1
    });
    expect(rate).toBe(1.0);
});

it("computePlaybackRate clamps to max adjust", () => {
    const lowRate = computePlaybackRate({
        latencySeconds: 0.0,
        targetLatencySeconds: 0.1,
        maxAdjust: 0.005,
        gain: 1.0
    });
    expect(lowRate).toBe(0.995);

    const highRate = computePlaybackRate({
        latencySeconds: 0.2,
        targetLatencySeconds: 0.1,
        maxAdjust: 0.005,
        gain: 1.0
    });
    expect(highRate).toBe(1.005);
});

it("computePlaybackRate scales within range", () => {
    const rate = computePlaybackRate({
        latencySeconds: 0.12,
        targetLatencySeconds: 0.1,
        maxAdjust: 0.005,
        gain: 0.1
    });
    expect(rate).toBe(1.002);
});
