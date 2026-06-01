import { expect, it } from "vitest";
import { AUDIO_PROFILES, resolveAudioProfileName } from "./audio_profiles.js";

it("resolveAudioProfileName defaults to balanced", () => {
    expect(resolveAudioProfileName(null)).toBe("balanced");
    expect(resolveAudioProfileName(undefined)).toBe("balanced");
    expect(resolveAudioProfileName("unknown")).toBe("balanced");
});

it("resolveAudioProfileName accepts the low-latency profile", () => {
    expect(resolveAudioProfileName("low-latency")).toBe("low-latency");
});

it("audio profiles expose the configured latency targets", () => {
    expect(AUDIO_PROFILES.balanced.targetLatencySeconds).toBe(0.06);
    expect(AUDIO_PROFILES.balanced.maxAdjust).toBe(0.0075);
    expect(AUDIO_PROFILES["low-latency"].targetLatencySeconds).toBe(0.04);
    expect(AUDIO_PROFILES["low-latency"].maxAdjust).toBe(0.01);
});
