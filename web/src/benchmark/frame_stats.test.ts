import { describe, expect, it } from "vitest";
import { computeFrameStats } from "./frame_stats";

describe("computeFrameStats", () => {
    it("calculates frame timing percentiles and effective FPS", () => {
        expect(computeFrameStats([12, 16, 20, 10, 18])).toEqual({
            frames: 5,
            totalMs: 76,
            averageMs: 15.2,
            p50Ms: 16,
            p95Ms: 20,
            maxMs: 20,
            fps: 65.78947368421052
        });
    });

    it("handles single-sample benchmark runs", () => {
        expect(computeFrameStats([17])).toEqual({
            frames: 1,
            totalMs: 17,
            averageMs: 17,
            p50Ms: 17,
            p95Ms: 17,
            maxMs: 17,
            fps: 58.8235294117647
        });
    });

    it("rejects empty sample sets", () => {
        expect(() => computeFrameStats([])).toThrow("frame timing samples must not be empty");
    });

    it("rejects zero-total sample sets", () => {
        expect(() => computeFrameStats([0])).toThrow("frame timing total must be greater than zero");
    });

    it.each([Number.NaN, Number.POSITIVE_INFINITY, -1])(
        "rejects invalid sample value %s",
        (sample) => {
            expect(() => computeFrameStats([sample])).toThrow(
                "frame timing samples must be finite non-negative numbers"
            );
        }
    );
});
