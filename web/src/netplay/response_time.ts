/** How a round trip between the two browsers feels, in the word and colour the players see. */
export type ResponseTimeRating =
    | { word: "Great"; tone: "ok" }
    | { word: "Playable"; tone: "warn" }
    | { word: "Too slow"; tone: "err" };

/** The agreed limits: under 50 ms is Great, up to 120 ms Playable, above that Too slow. */
export const GREAT_BELOW_MS = 50;
export const PLAYABLE_UP_TO_MS = 120;

export function responseTimeRating(ms: number): ResponseTimeRating {
    if (ms < GREAT_BELOW_MS) return { word: "Great", tone: "ok" };
    if (ms <= PLAYABLE_UP_TO_MS) return { word: "Playable", tone: "warn" };
    return { word: "Too slow", tone: "err" };
}

export function isTooSlow(ms: number): boolean {
    return ms > PLAYABLE_UP_TO_MS;
}

export function median(samples: readonly number[]): number {
    const sorted = [...samples].sort((a, b) => a - b);
    const middle = Math.floor(sorted.length / 2);
    return sorted.length % 2 === 1 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
}
