/** The volume slider's value, in whole percent, when nothing is stored yet. */
export const DEFAULT_VOLUME = 100;

/** Where the slider's value is kept between visits. */
export const VOLUME_STORAGE_KEY = "neser.volume";

/** The gain for a slider percent: squared, so the lower half of the slider stays usable. */
export function volumeToGain(percent: number): number {
    const fraction = Math.min(1, Math.max(0, percent / 100));
    return fraction * fraction;
}

/** A stored slider value read back: a whole percent 0..100, or the default when absent or garbled. */
export function parseStoredVolume(stored: string | null): number {
    if (stored === null || stored.trim() === "") return DEFAULT_VOLUME;
    const value = Number(stored);
    if (!Number.isFinite(value) || value < 0 || value > 100) return DEFAULT_VOLUME;
    return Math.round(value);
}

/** The remembered volume; the default when nothing is stored or the browser refuses storage. */
export function readStoredVolume(storage: Pick<Storage, "getItem"> | null): number {
    try {
        return parseStoredVolume(storage?.getItem(VOLUME_STORAGE_KEY) ?? null);
    } catch {
        return DEFAULT_VOLUME;
    }
}

/** Remembers the volume; a browser that refuses storage keeps it for this visit only. */
export function writeStoredVolume(storage: Pick<Storage, "setItem"> | null, percent: number): void {
    try {
        storage?.setItem(VOLUME_STORAGE_KEY, String(percent));
    } catch {
        // Private windows may refuse storage; the slider still works for this visit.
    }
}

/** Which speaker the mute button shows: crossed out when muted or turned all the way down. */
export function volumeIconState(muted: boolean, percent: number): "on" | "off" {
    return muted || percent === 0 ? "off" : "on";
}
