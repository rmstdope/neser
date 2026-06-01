import { describe, expect, it } from "vitest";
import { parseGbaBenchmarkConfig } from "./gba_benchmark_config";

describe("parseGbaBenchmarkConfig", () => {
    it("uses Metroid benchmark defaults when only the ROM is provided", () => {
        const config = parseGbaBenchmarkConfig(new URLSearchParams("rom=metroid-zero-mission.gba"));

        expect(config).toEqual({
            romName: "metroid-zero-mission.gba",
            frames: 600,
            warmupFrames: 60,
            stabilityRuns: 5,
            resetStabilityRuns: true,
            skipBiosIntro: true
        });
    });

    it("parses frame, warmup, and stability overrides", () => {
        const config = parseGbaBenchmarkConfig(
            new URLSearchParams("rom=test.gba&frames=120&warmup=30&stabilityRuns=2")
        );

        expect(config).toEqual({
            romName: "test.gba",
            frames: 120,
            warmupFrames: 30,
            stabilityRuns: 2,
            resetStabilityRuns: true,
            skipBiosIntro: true
        });
    });

    it("rejects missing ROM parameter", () => {
        expect(() => parseGbaBenchmarkConfig(new URLSearchParams())).toThrow(
            "Missing ?rom=<file>. Provide a GBA ROM filename in web/roms/."
        );
    });

    it("rejects unsafe ROM names", () => {
        expect(() => parseGbaBenchmarkConfig(new URLSearchParams("rom=../metroid.gba"))).toThrow(
            "Invalid ROM name. Only letters, numbers, dot, underscore, or dash allowed."
        );
    });

    it.each(["0", "-1", "10.5", "abc"])(
        "rejects invalid positive frame value %s",
        (frames) => {
            expect(() =>
                parseGbaBenchmarkConfig(new URLSearchParams(`rom=test.gba&frames=${frames}`))
            ).toThrow("Invalid frames value: expected a positive integer.");
        }
    );

    it.each([
        ["warmup", "-1"],
        ["warmup", "10.5"],
        ["warmup", "abc"],
        ["stabilityRuns", "-1"],
        ["stabilityRuns", "10.5"],
        ["stabilityRuns", "abc"]
    ])("rejects invalid non-negative %s value %s", (key, value) => {
        expect(() =>
            parseGbaBenchmarkConfig(new URLSearchParams(`rom=test.gba&${key}=${value}`))
        ).toThrow(`Invalid ${key} value: expected a non-negative integer.`);
    });

    it("allows zero warmup and stability runs", () => {
        const config = parseGbaBenchmarkConfig(
            new URLSearchParams("rom=test.gba&warmup=0&stabilityRuns=0")
        );

        expect(config.warmupFrames).toBe(0);
        expect(config.stabilityRuns).toBe(0);
    });

    it("can continue stability runs without resetting", () => {
        const config = parseGbaBenchmarkConfig(
            new URLSearchParams("rom=test.gba&continueStabilityRuns=true")
        );

        expect(config.resetStabilityRuns).toBe(false);
    });

    it("can include BIOS intro in benchmark runs", () => {
        const config = parseGbaBenchmarkConfig(
            new URLSearchParams("rom=test.gba&includeBiosIntro=true")
        );

        expect(config.skipBiosIntro).toBe(false);
    });
});
