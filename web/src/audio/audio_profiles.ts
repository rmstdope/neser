export type AudioProfileName = "balanced" | "low-latency";

export type AudioProfile = {
    targetLatencySeconds: number;
    maxAdjust: number;
};

export const AUDIO_PROFILES: Record<AudioProfileName, AudioProfile> = {
    balanced: {
        targetLatencySeconds: 0.06,
        maxAdjust: 0.0075
    },
    "low-latency": {
        targetLatencySeconds: 0.04,
        maxAdjust: 0.01
    }
};

export function resolveAudioProfileName(value: string | null | undefined): AudioProfileName {
    return value === "low-latency" ? "low-latency" : "balanced";
}
