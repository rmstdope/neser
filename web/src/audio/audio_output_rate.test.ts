import { describe, expect, it } from "vitest";
import { configureEmulatorAudioSampleRate, shouldConfigureAudioContextSampleRate } from "./audio_output_rate";

describe("configureEmulatorAudioSampleRate", () => {
    it("propagates the AudioContext sample rate to emulator audio output", () => {
        const calls: number[] = [];

        const configured = configureEmulatorAudioSampleRate(
            { set_audio_sample_rate: (sampleRate) => calls.push(sampleRate) },
            48_000
        );

        expect(configured).toBe(true);
        expect(calls).toEqual([48_000]);
    });

    it("returns false when the emulator cannot accept sample-rate changes", () => {
        expect(configureEmulatorAudioSampleRate({}, 48_000)).toBe(false);
    });

    it("does not apply invalid sample rates", () => {
        const calls: number[] = [];
        const emulator = { set_audio_sample_rate: (sampleRate: number) => calls.push(sampleRate) };

        expect(configureEmulatorAudioSampleRate(emulator, 0)).toBe(false);
        expect(configureEmulatorAudioSampleRate(emulator, -1)).toBe(false);
        expect(configureEmulatorAudioSampleRate(emulator, Number.NaN)).toBe(false);
        expect(configureEmulatorAudioSampleRate(emulator, Number.POSITIVE_INFINITY)).toBe(false);
        expect(calls).toEqual([]);
    });

    it("configures GBA audio generation to the browser output rate like native", () => {
        expect(shouldConfigureAudioContextSampleRate("gba")).toBe(true);
        expect(shouldConfigureAudioContextSampleRate("nes")).toBe(true);
        expect(shouldConfigureAudioContextSampleRate("gb")).toBe(true);
    });
});
