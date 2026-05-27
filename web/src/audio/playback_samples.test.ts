import { describe, expect, it } from "vitest";
import { getPlaybackAudioSamples } from "./playback_samples";

describe("getPlaybackAudioSamples", () => {
    it("uses interleaved stereo samples for GBA when available", () => {
        const monoCalls: string[] = [];
        const stereoSamples = new Float32Array([0.25, -0.25, 0.5, -0.5]);

        const playback = getPlaybackAudioSamples("gba", {
            get_audio_samples: () => {
                monoCalls.push("mono");
                return new Float32Array([0.0, 0.0]);
            },
            get_audio_samples_stereo: () => stereoSamples
        });

        expect(playback.channels).toBe(2);
        expect(playback.samples).toBe(stereoSamples);
        expect(monoCalls).toEqual([]);
    });

    it("keeps NES and GB playback mono", () => {
        const nesSamples = new Float32Array([0.1, 0.2]);
        const gbSamples = new Float32Array([-0.1, 0.1]);

        expect(
            getPlaybackAudioSamples("nes", {
                get_audio_samples: () => nesSamples,
                get_audio_samples_stereo: () => new Float32Array([1, 1])
            })
        ).toEqual({ channels: 1, samples: nesSamples });
        expect(
            getPlaybackAudioSamples("gb", {
                get_audio_samples: () => gbSamples,
                get_audio_samples_stereo: () => new Float32Array([1, 1])
            })
        ).toEqual({ channels: 1, samples: gbSamples });
    });
});
