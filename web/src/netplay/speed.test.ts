import { describe, expect, it } from "vitest";
import { createSpeedWatch, type SpeedWatch } from "./speed";

const NES_FPS = 60.0988;

/**
 * Animation frames every `interval` ms for `ms`, from `from`, each running `frames` game frames,
 * or only every `runEvery`th one does (a frame limiter running the game slower on purpose).
 */
function drive(watch: SpeedWatch, { from, ms, interval, frames = 1, runEvery = 1 }: { from: number; ms: number; interval: number; frames?: number; runEvery?: number }) {
    let t = from;
    let n = 0;
    for (; t < from + ms; t += interval) {
        watch.animationFrame(t);
        if (n++ % runEvery === 0) watch.ran(frames);
    }
    return t;
}

const watch = () => createSpeedWatch(() => NES_FPS);

describe("speed watch", () => {
    it("wants nothing from a computer at full speed on a 60 Hz display", () => {
        const w = watch();
        drive(w, { from: 0, ms: 10_000, interval: 1000 / 60 });
        expect(w.wanted()).toBeNull();
    });

    it("wants 95 % of what it managed once it has stayed below full speed for three seconds", () => {
        const w = watch();
        let t = drive(w, { from: 0, ms: 2500, interval: 25 });
        expect(w.wanted()).toBeNull();
        t = drive(w, { from: t, ms: 1000, interval: 25 });
        expect(w.wanted()).toBe(38);
    });

    it("wants nothing for a short slow patch", () => {
        const w = watch();
        let t = drive(w, { from: 0, ms: 2000, interval: 25 });
        t = drive(w, { from: t, ms: 2000, interval: 1000 / 60 });
        drive(w, { from: t, ms: 2000, interval: 25 });
        expect(w.wanted()).toBeNull();
    });

    it("counts two frames run at once as what they cost each", () => {
        const w = watch();
        // Catching up two frames an animation frame, 30 animation frames a second: 60 frames a second.
        drive(w, { from: 0, ms: 5000, interval: 1000 / 30, frames: 2 });
        expect(w.wanted()).toBeNull();
    });

    it("wants full speed again after three seconds back at it", () => {
        const w = watch();
        let t = drive(w, { from: 0, ms: 4000, interval: 25 });
        expect(w.wanted()).toBe(38);
        t = drive(w, { from: t, ms: 2000, interval: 1000 / 60 });
        expect(w.wanted()).toBe(38);
        drive(w, { from: t, ms: 1500, interval: 1000 / 60 });
        expect(w.wanted()).toBeNull();
    });

    it("does not take frames skipped on purpose for slowness", () => {
        const w = watch();
        let t = drive(w, { from: 0, ms: 4000, interval: 25 });
        expect(w.wanted()).toBe(38);
        // Running slower together, the limiter skips some animation frames; this one is still slow.
        t = drive(w, { from: t, ms: 6000, interval: 25, runEvery: 3 });
        expect(w.wanted()).toBe(38);
        // A computer that is fast again, running only some of its animation frames, is fast.
        drive(w, { from: t, ms: 4000, interval: 1000 / 60, runEvery: 2 });
        expect(w.wanted()).toBeNull();
    });

    it("wants less again when it cannot keep even the slower speed", () => {
        const w = watch();
        const t = drive(w, { from: 0, ms: 4000, interval: 25 });
        expect(w.wanted()).toBe(38);
        drive(w, { from: t, ms: 3500, interval: 1000 / 30 });
        expect(w.wanted()).toBe(28);
    });

    it("does not take a long gap (a hidden page) for slowness", () => {
        const w = watch();
        let t = drive(w, { from: 0, ms: 1000, interval: 1000 / 60 });
        for (let i = 0; i < 4; i++) {
            t = drive(w, { from: t + 2000, ms: 500, interval: 1000 / 60 });
        }
        expect(w.wanted()).toBeNull();
    });

    it("does not count seconds the game mostly held, waiting for the other player", () => {
        const w = watch();
        // Two slow seconds, a held one, then two slow seconds more: never three in a row.
        let t = drive(w, { from: 0, ms: 2000, interval: 25 });
        t = drive(w, { from: t, ms: 1000, interval: 1000 / 60, runEvery: 1000 });
        drive(w, { from: t, ms: 2000, interval: 25 });
        expect(w.wanted()).toBeNull();
    });

    it("starts afresh once reset", () => {
        const w = watch();
        const t = drive(w, { from: 0, ms: 2500, interval: 25 });
        w.reset();
        drive(w, { from: t, ms: 1500, interval: 25 });
        expect(w.wanted()).toBeNull();
    });
});
