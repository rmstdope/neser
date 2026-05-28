export interface FrameStats {
    frames: number;
    totalMs: number;
    averageMs: number;
    p50Ms: number;
    p95Ms: number;
    maxMs: number;
    fps: number;
}

export function computeFrameStats(samplesMs: readonly number[]): FrameStats {
    if (samplesMs.length === 0) {
        throw new Error("frame timing samples must not be empty");
    }
    if (samplesMs.some((sample) => !Number.isFinite(sample) || sample < 0)) {
        throw new Error("frame timing samples must be finite non-negative numbers");
    }

    const sorted = [...samplesMs].sort((a, b) => a - b);
    const totalMs = samplesMs.reduce((sum, sample) => sum + sample, 0);
    if (totalMs === 0) {
        throw new Error("frame timing total must be greater than zero");
    }
    const averageMs = totalMs / samplesMs.length;

    return {
        frames: samplesMs.length,
        totalMs,
        averageMs,
        p50Ms: percentile(sorted, 0.50),
        p95Ms: percentile(sorted, 0.95),
        maxMs: sorted[sorted.length - 1],
        fps: samplesMs.length * 1000 / totalMs
    };
}

function percentile(sortedSamples: readonly number[], percentileValue: number): number {
    const rank = Math.ceil(percentileValue * sortedSamples.length);
    return sortedSamples[Math.min(Math.max(rank - 1, 0), sortedSamples.length - 1)];
}
