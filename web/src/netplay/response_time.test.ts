import { describe, expect, it } from "vitest";
import { median, responseTimeRating } from "./response_time";

describe("the response time's word", () => {
    it.each([
        [0, "Great", "ok"],
        [49, "Great", "ok"],
        [50, "Playable", "warn"],
        [120, "Playable", "warn"],
        [121, "Too slow", "err"],
        [400, "Too slow", "err"],
    ])("%i ms is %s", (ms, word, tone) => {
        expect(responseTimeRating(ms)).toEqual({ word, tone });
    });
});

describe("the median of round trips", () => {
    it("is the middle one of an odd count", () => {
        expect(median([90, 10, 20])).toBe(20);
    });

    it("is the mean of the middle two of an even count", () => {
        expect(median([10, 40, 20, 30])).toBe(25);
    });
});
