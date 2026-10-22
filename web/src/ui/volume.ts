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
    if (stored === null) return DEFAULT_VOLUME;
    const value = Number(stored);
    if (!Number.isFinite(value) || value < 0 || value > 100) return DEFAULT_VOLUME;
    return Math.round(value);
}

/** Which speaker the mute button shows: crossed out when muted or turned all the way down. */
export function volumeIconState(muted: boolean, percent: number): "on" | "off" {
    return muted || percent === 0 ? "off" : "on";
}
