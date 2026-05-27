export interface AudioSampleRateConfigurable {
    set_audio_sample_rate?(sampleRate: number): void;
}

export function configureEmulatorAudioSampleRate(
    emulator: AudioSampleRateConfigurable,
    sampleRate: number
): boolean {
    if (!Number.isFinite(sampleRate) || sampleRate <= 0) {
        return false;
    }
    if (typeof emulator.set_audio_sample_rate !== "function") {
        return false;
    }
    emulator.set_audio_sample_rate(sampleRate);
    return true;
}
