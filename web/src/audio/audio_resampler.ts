export function computePlaybackRate({
    latencySeconds,
    targetLatencySeconds,
    maxAdjust = 0.005,
    gain = 1.0
}: {
    latencySeconds: number;
    targetLatencySeconds: number;
    maxAdjust?: number;
    gain?: number;
}) {
    if (targetLatencySeconds <= 0 || maxAdjust <= 0 || !Number.isFinite(latencySeconds)) {
        return 1.0;
    }

    const delta = latencySeconds - targetLatencySeconds;
    const adjustment = Math.max(-maxAdjust, Math.min(maxAdjust, delta * gain));
    return 1.0 + adjustment;
}
