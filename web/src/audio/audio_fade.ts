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
/** How far a pending fade-out starts after a fade-in has finished, so cancelling it never touches the fade-in. */
const APART = 1e-4;

export function createAudioFade(param: FadeParam) {
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
                const length = Math.min(FADE_SECONDS, (end - start) / 2);
                param.setValueAtTime(0, start);
                param.linearRampToValueAtTime(1, start + length);
                fadeInEnd = start + length;
            }
            fadeOutStart = Math.max(end - FADE_SECONDS, fadeInEnd + APART);
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
