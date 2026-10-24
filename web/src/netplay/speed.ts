/**
 * Whether this computer can keep the game at full speed while playing together (nr-ayk5.7,
 * docs/ui/nr-ayk5.7-smooth-play.html), and if not, what speed it can keep: both games then run at
 * that speed together.
 *
 * What a frame costs is the time from the animation frame that ran it to the next animation
 * frame, shared among the frames it ran. Measured per animation frame rather than per frame, it
 * reads the same whether the game runs every animation frame or, running slower on purpose, skips
 * some. Each second gives this computer's speed; three in a row noticeably below the speed it now
 * plays at make it want a little less than it managed, and three in a row back at full speed make
 * it want full speed again.
 */

/** A second's speed below this share of the speed played at is slow. */
export const SLOW_BELOW = 0.9;
/** A second's speed at or above this share of full speed is full. */
export const FULL_FROM = 0.97;
/** The share of what a slow computer managed that both games then run at, leaving it some room. */
export const HEADROOM = 0.95;
/** How many seconds in a row decide. */
const SECONDS_TO_DECIDE = 3;
const WINDOW_MS = 1000;
/**
 * A second whose frames took less time than this to run was mostly held, waiting for the other
 * player: it says nothing about speed. Told apart by time, not by frames, since a very slow
 * computer runs few frames too, but they take the whole second.
 */
const ENOUGH_MS = 250;
/** A longer wait between two animation frames is the page hidden or a stall, not speed. */
const LONGEST_FRAME_MS = 250;

export function createSpeedWatch(fullSpeed: () => number) {
    let wanted: number | null = null;
    /** The animation frame before this one, and the frames it ran. */
    let previous: { t: number; frames: number } | null = null;
    let windowStart: number | null = null;
    let frames = 0;
    let ms = 0;
    let slow = 0;
    let full = 0;

    function judge() {
        const target = wanted ?? fullSpeed();
        if (frames === 0 || ms < ENOUGH_MS) {
            slow = 0;
            full = 0;
            return;
        }
        const speed = (frames * 1000) / ms;
        slow = speed < SLOW_BELOW * target ? slow + 1 : 0;
        full = speed >= FULL_FROM * fullSpeed() ? full + 1 : 0;
        if (slow >= SECONDS_TO_DECIDE) {
            slow = 0;
            const less = Math.floor(speed * HEADROOM);
            if (less >= 1 && less < fullSpeed()) wanted = less;
        } else if (full >= SECONDS_TO_DECIDE && wanted !== null) {
            full = 0;
            wanted = null;
        }
    }

    return {
        /** An animation frame came at `t` (ms). */
        animationFrame(t: number) {
            if (previous && previous.frames > 0) {
                const cost = t - previous.t;
                if (cost >= 0 && cost <= LONGEST_FRAME_MS) {
                    frames += previous.frames;
                    ms += cost;
                }
            }
            previous = { t, frames: 0 };
            windowStart ??= t;
            if (t - windowStart >= WINDOW_MS) {
                judge();
                windowStart = t;
                frames = 0;
                ms = 0;
            }
        },
        /** The current animation frame ran `count` frames. */
        ran(count: number) {
            if (previous) previous.frames += count;
        },
        /** The game stopped (paused, counting, resyncing): what it measured so far no longer counts. */
        reset() {
            previous = null;
            windowStart = null;
            frames = 0;
            ms = 0;
            slow = 0;
            full = 0;
        },
        /** The frames a second this computer can keep, or null when it keeps full speed. */
        wanted: () => wanted,
    };
}

export type SpeedWatch = ReturnType<typeof createSpeedWatch>;
