import type { PlayView } from "../netplay/play";

/**
 * The note that says why both games run slower (nr-ayk5.7, docs/ui/nr-ayk5.7-smooth-play.html):
 * shown at the bottom of the game for five seconds, once each time players play together, worded
 * for this side. It goes as soon as the game stops (paused, a window, a count, a resync, the end)
 * and does not come back; it never takes focus, being a toast.
 */

export const SLOW_NOTES = {
    me: "Your computer can't keep full speed, so both games run a little slower.",
    them: "The other player's computer can't keep full speed, so both games run a little slower.",
} as const;
export const SLOW_NOTE_MS = 5000;

/** `show` puts a message up for a time and returns what takes it away at once. */
export function createSlowNote(show: (message: string, ms: number) => () => void) {
    let shown = false;
    let dismiss: (() => void) | null = null;

    function takeAway() {
        dismiss?.();
        dismiss = null;
    }

    return {
        /** Each new view of the game played together. */
        draw(view: Pick<PlayView, "phase" | "slowNote">) {
            if (view.phase !== "playing") return takeAway();
            if (view.slowNote === null || shown) return;
            shown = true;
            dismiss = show(SLOW_NOTES[view.slowNote], SLOW_NOTE_MS);
        },
        /** Playing together is over: the note goes, and may show again the next time. */
        end() {
            takeAway();
            shown = false;
        },
    };
}
