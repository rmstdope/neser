export interface GbaBenchmarkConfig {
    romName: string;
    frames: number;
    warmupFrames: number;
    stabilityRuns: number;
    resetStabilityRuns: boolean;
    skipBiosIntro: boolean;
}

const SAFE_ROM_BASENAME = /^[A-Za-z0-9._-]+$/;

export function parseGbaBenchmarkConfig(params: URLSearchParams): GbaBenchmarkConfig {
    const romName = params.get("rom");
    if (!romName) {
        throw new Error("Missing ?rom=<file>. Provide a GBA ROM filename in web/roms/.");
    }
    if (!SAFE_ROM_BASENAME.test(romName)) {
        throw new Error("Invalid ROM name. Only letters, numbers, dot, underscore, or dash allowed.");
    }

    return {
        romName,
        frames: parsePositiveInteger(params, "frames", 600),
        warmupFrames: parseNonNegativeInteger(params, "warmup", 60),
        stabilityRuns: parseNonNegativeInteger(params, "stabilityRuns", 5),
        resetStabilityRuns: params.get("continueStabilityRuns") !== "true",
        skipBiosIntro: params.get("includeBiosIntro") !== "true"
    };
}

function parsePositiveInteger(
    params: URLSearchParams,
    key: string,
    defaultValue: number
): number {
    const value = params.get(key);
    if (value === null) return defaultValue;
    const parsed = Number(value);
    if (!Number.isInteger(parsed) || parsed <= 0) {
        throw new Error(`Invalid ${key} value: expected a positive integer.`);
    }
    return parsed;
}

function parseNonNegativeInteger(
    params: URLSearchParams,
    key: string,
    defaultValue: number
): number {
    const value = params.get(key);
    if (value === null) return defaultValue;
    const parsed = Number(value);
    if (!Number.isInteger(parsed) || parsed < 0) {
        throw new Error(`Invalid ${key} value: expected a non-negative integer.`);
    }
    return parsed;
}
