/**
 * Sound that fades instead of clicking (nr-ayk5.7, docs/ui/nr-ayk5.7-smooth-play.html): while
 * playing together the game can hold for a moment, pause, count or resync, and the sound scheduled
 * so far then simply runs out. This keeps a fade-out pending that ends exactly where the scheduled
 * sound ends, and moves it later whenever a chunk carries on from there before it has begun; a
 * chunk after a gap, or one handed over once the fade-out has begun, fades in from silence. It
 * needs no timer and no knowledge of why the sound stopped: whatever stops it, it fades.
 *
 * Only events in the future are ever cancelled, and never the end of a ramp under way: the
 * pending fade-out always starts after the last fade-in has finished, and is only moved while it
 * has not yet begun.
 */

/** The part of a gain `AudioParam` the fade drives. */
export interface FadeParam {
    setValueAtTime(value: number, time: number): unknown;
    linearRampToValueAtTime(value: number, time: number): unknown;
    cancelScheduledValues(time: number): unknown;
}

/** How long a fade takes: a few hundredths of a second, as agreed. */
export const FADE_SECONDS = 0.02;
/** The time one frame takes, at about 60 frames a second. */
const FRAME_SECONDS = 1 / 60;
/** What the audio clock and animation frames may wander by, on top of one frame. */
const JITTER_SECONDS = 0.01;
/** The shortest fade that still does not click. */
const SHORTEST_FADE_SECONDS = 0.005;

/**
 * The fade for a sound lead of `leadSeconds`: one frame held, handed over a little late, must
 * still come before the pending fade-out begins, or every one-frame correction between two
 * computers would dip. The balanced lead (60 ms) leaves room for the full fade; the low-latency
 * one (40 ms) needs a quicker one.
 */
export function fadeSecondsFor(leadSeconds: number): number {
    const room = leadSeconds - FRAME_SECONDS - JITTER_SECONDS;
    return Math.min(FADE_SECONDS, Math.max(SHORTEST_FADE_SECONDS, room));
}

/** How far a pending fade-out starts after a fade-in has finished, so cancelling it never touches the fade-in. */
const APART = 1e-4;

export function createAudioFade(param: FadeParam, fadeSeconds = FADE_SECONDS) {
    /** Where the sound scheduled so far ends, and with it the pending fade-out. */
    let soundEnd = -Infinity;
    /** Where the pending fade-out begins. */
    let fadeOutStart = -Infinity;
    /** Where the last fade-in finished. */
    let fadeInEnd = -Infinity;

    return {
        /** A chunk of sound handed over at `now`, playing from `start` until `end` (all in audio time). */
        chunk(now: number, start: number, end: number) {
            const carriesOn = start <= soundEnd + APART;
            if (carriesOn && now < fadeOutStart) {
                param.cancelScheduledValues(fadeOutStart);
            } else {
                // After a gap, or the sound so far is already fading out (it reaches silence at
                // `start`): this chunk fades in, within its first half at most.
                const length = Math.min(fadeSeconds, (end - start) / 2);
                param.setValueAtTime(0, start);
                param.linearRampToValueAtTime(1, start + length);
                fadeInEnd = start + length;
            }
            fadeOutStart = Math.max(end - fadeSeconds, fadeInEnd + APART);
            soundEnd = end;
            param.setValueAtTime(1, fadeOutStart);
            param.linearRampToValueAtTime(0, soundEnd);
        },
        /** Full gain from `now` on, with nothing pending: playing together is over. */
        reset(now: number) {
            param.cancelScheduledValues(now);
            param.setValueAtTime(1, now);
            soundEnd = -Infinity;
            fadeOutStart = -Infinity;
            fadeInEnd = -Infinity;
        },
    };
}

export type AudioFade = ReturnType<typeof createAudioFade>;
