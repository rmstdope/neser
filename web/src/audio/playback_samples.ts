import { CONSOLES, type ConsoleKind } from "../console/consoles";

export interface AudioPlaybackSampleSource {
    get_audio_samples(): Float32Array;
    get_audio_samples_stereo?(): Float32Array;
}

export interface PlaybackAudioSamples {
    channels: number;
    samples: Float32Array;
}

export function getPlaybackAudioSamples(
    consoleKind: ConsoleKind,
    source: AudioPlaybackSampleSource
): PlaybackAudioSamples {
    if (CONSOLES[consoleKind].audio.stereo && typeof source.get_audio_samples_stereo === "function") {
        return {
            channels: 2,
            samples: source.get_audio_samples_stereo()
        };
    }

    return {
        channels: 1,
        samples: source.get_audio_samples()
    };
}
