import { describe, expect, it } from "vitest";
import { createAudioFade, FADE_SECONDS, type FadeParam } from "./audio_fade";

/**
 * A gain `AudioParam` as Web Audio automates it: values set at a time hold until the next event,
 * a linear ramp runs from the previous event's time and value to its own, and cancelling removes
 * every event at or after the given time.
 */
class Gain implements FadeParam {
    private events: { kind: "set" | "ramp"; time: number; value: number }[] = [];
    setValueAtTime(value: number, time: number) {
        this.add({ kind: "set", time, value });
    }
    linearRampToValueAtTime(value: number, time: number) {
        this.add({ kind: "ramp", time, value });
    }
    cancelScheduledValues(time: number) {
        this.events = this.events.filter((event) => event.time < time);
    }
    private add(event: { kind: "set" | "ramp"; time: number; value: number }) {
        // Events at the same time keep the order they were added in.
        const at = this.events.findIndex((other) => other.time > event.time);
        if (at < 0) this.events.push(event);
        else this.events.splice(at, 0, event);
    }
    at(time: number): number {
        let before = { time: 0, value: 1 };
        for (const event of this.events) {
            if (event.time <= time) {
                before = event;
                continue;
            }
            if (event.kind === "ramp") {
                const span = event.time - before.time;
                return span <= 0 ? event.value : before.value + ((event.value - before.value) * (time - before.time)) / span;
            }
            break;
        }
        return before.value;
    }
}

const CHUNK = 1 / 60;
const LEAD = 0.06;
const F = FADE_SECONDS;

/** Chunks of sound scheduled back to back, each handed over `LEAD` before it plays. */
function playChunks(fade: ReturnType<typeof createAudioFade>, from: number, count: number): number {
    let start = from;
    for (let i = 0; i < count; i++) {
        fade.chunk(start - LEAD, start, start + CHUNK);
        start += CHUNK;
    }
    return start;
}

describe("audio fade", () => {
    it("fades over a few hundredths of a second", () => {
        expect(F).toBeGreaterThanOrEqual(0.01);
        expect(F).toBeLessThanOrEqual(0.03);
    });

    it("keeps full gain while chunks carry on from each other", () => {
        const gain = new Gain();
        const fade = createAudioFade(gain);
        const end = playChunks(fade, 1, 60);
        for (let t = 1 + F; t < end - LEAD; t += 0.001) expect(gain.at(t)).toBeCloseTo(1, 6);
    });

    it("fades to silence exactly where the scheduled sound ends when nothing follows", () => {
        const gain = new Gain();
        const fade = createAudioFade(gain);
        const end = playChunks(fade, 1, 30);
        expect(gain.at(end - F - 0.0001)).toBeCloseTo(1, 3);
        expect(gain.at(end - F / 2)).toBeCloseTo(0.5, 1);
        expect(gain.at(end)).toBe(0);
        expect(gain.at(end + 1)).toBe(0);
    });

    it("fades in a chunk that starts after a gap", () => {
        const gain = new Gain();
        const fade = createAudioFade(gain);
        const end = playChunks(fade, 1, 10);
        const later = end + 0.3;
        playChunks(fade, later, 10);
        expect(gain.at(end + 0.1)).toBe(0);
        expect(gain.at(later)).toBe(0);
        // A fade fits in the chunk it starts: at most half of a 60 fps frame's sound.
        expect(gain.at(later + 0.004)).toBeGreaterThan(0.3);
        expect(gain.at(later + 0.004)).toBeLessThan(0.7);
        expect(gain.at(later + F)).toBeCloseTo(1, 6);
    });

    it("dips and fades back in when a chunk arrives after the fade-out began", () => {
        const gain = new Gain();
        const fade = createAudioFade(gain);
        const end = playChunks(fade, 1, 10);
        // Handed over only 5 ms before the sound so far runs out: its last 20 ms are already fading.
        fade.chunk(end - 0.005, end, end + CHUNK);
        fade.chunk(end - 0.004, end + CHUNK, end + 2 * CHUNK);
        expect(gain.at(end)).toBe(0);
        // Back at full gain until the last chunk's own fade-out, since nothing follows it.
        expect(gain.at(end + 2 * CHUNK - F - 0.0001)).toBeCloseTo(1, 3);
        expect(gain.at(end + 2 * CHUNK)).toBe(0);
    });

    it("does not dip for a frame handed over late while the sound so far still plays", () => {
        // One frame held, as when the faster of two computers waits a tick for the slower one.
        const gain = new Gain();
        const fade = createAudioFade(gain);
        const end = playChunks(fade, 1, 30);
        fade.chunk(end - LEAD + CHUNK, end, end + CHUNK);
        const after = playChunks(fade, end + CHUNK, 30);
        for (let t = 1 + F; t < after - LEAD; t += 0.001) expect(gain.at(t)).toBeCloseTo(1, 6);
    });

    it("fades a lone short chunk in and out within it", () => {
        const gain = new Gain();
        const fade = createAudioFade(gain);
        fade.chunk(0.9, 1, 1 + CHUNK);
        expect(gain.at(1)).toBe(0);
        expect(gain.at(1 + CHUNK / 2)).toBeCloseTo(1, 3);
        expect(gain.at(1 + CHUNK)).toBe(0);
    });

    it("never jumps by more than a fade step between two samples", () => {
        const gain = new Gain();
        const fade = createAudioFade(gain);
        // Steady play, a short hold, play on, a late chunk, a long silence and play again.
        let end = playChunks(fade, 1, 20);
        end = playChunks(fade, end + 0.05, 5);
        fade.chunk(end - 0.002, end, end + CHUNK);
        end = playChunks(fade, end + CHUNK, 5);
        playChunks(fade, end + 1, 5);
        const step = 1 / 44100;
        let last = gain.at(1);
        for (let t = 1; t < end + 1.2; t += step) {
            const now = gain.at(t);
            expect(Math.abs(now - last)).toBeLessThan((2 * step) / (F / 2));
            last = now;
        }
    });

    it("goes back to full gain, with nothing pending, once reset", () => {
        const gain = new Gain();
        const fade = createAudioFade(gain);
        const end = playChunks(fade, 1, 10);
        fade.reset(end - 0.03);
        expect(gain.at(end)).toBe(1);
        expect(gain.at(end + 1)).toBe(1);
    });
});
