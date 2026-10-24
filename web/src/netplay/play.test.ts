import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createPlay, type CountLabel, type LockstepRules, type Play, type PlayCore } from "./play";
import type { PlayData, PlayMessage } from "./session";

// Playing together (nr-ayk5.5): two play controllers, the host's and the joiner's, over an in-memory
// link with a fixed one-way delay, each with a stand-in for the Rust lockstep that follows its
// rules (a frame runs only once both pads for it are known; the first `delay` frames run on
// released pads) and a stand-in core that records what it was asked to play.

const ONE_WAY_MS = 10;
const FRAME_MS = 1000 / 60;
const DELAY = 2;

/** The lockstep rules, as `platform::netplay::lockstep` has them, without the wire format. */
class Rules implements LockstepRules {
    private local: number[];
    private remote: (number | undefined)[];
    private next = 0;
    constructor(
        private player: 1 | 2,
        private delay: number
    ) {
        this.local = Array(delay).fill(0);
        this.remote = Array(delay).fill(0);
    }
    advance(pad: number): number[] {
        const remote = this.remote[this.next];
        if (remote === undefined) return [];
        const local = this.local[this.next];
        this.local.push(pad);
        const pads = this.player === 1 ? [local, remote] : [remote, local];
        return [this.next++, ...pads];
    }
    packet(): Uint8Array {
        return new TextEncoder().encode(JSON.stringify(this.local));
    }
    receive(bytes: Uint8Array): boolean {
        const pads = JSON.parse(new TextDecoder().decode(bytes)) as number[];
        pads.forEach((pad, frame) => {
            this.remote[frame] ??= pad;
        });
        return true;
    }
    next_frame() {
        return this.next;
    }
    remote_frames_known() {
        let known = 0;
        while (this.remote[this.next + known] !== undefined) known++;
        return known;
    }
    input_delay() {
        return this.delay;
    }
}

type Played = { frame: number; pads: [number, number] } | { reset: boolean; afterFrames: number };

class Core implements PlayCore {
    played: Played[] = [];
    frames = 0;
    runFrame(pad1: number, pad2: number) {
        this.played.push({ frame: this.frames++, pads: [pad1, pad2] });
    }
    reset(soft: boolean) {
        this.played.push({ reset: soft, afterFrames: this.frames });
    }
    pads() {
        return this.played.filter((p): p is { frame: number; pads: [number, number] } => "pads" in p).map((p) => p.pads);
    }
    resets() {
        return this.played.filter((p): p is { reset: boolean; afterFrames: number } => "reset" in p);
    }
}

/** One side: its controller, its core, the counts it showed, and whether its page is hidden. */
type Side = { play: Play; core: Core; counts: CountLabel[]; hidden: boolean; tickEvery: number; lastTick: number };

let host: Side;
let join: Side;

/** In order, `ONE_WAY_MS` later, as an ordered data channel delivers. */
function deliverer(to: () => Side) {
    let lastAt = 0;
    return (data: PlayData) => {
        const at = Math.max(performance.now() + ONE_WAY_MS, lastAt);
        lastAt = at;
        setTimeout(() => {
            const side = to();
            if (data instanceof Uint8Array) side.play.receive(data);
            else side.play.message(data);
        }, at - performance.now());
    };
}

function side(player: 1 | 2, other: () => Side): Side {
    const core = new Core();
    const counts: CountLabel[] = [];
    const send = deliverer(other);
    const play = createPlay({
        player,
        lockstep: new Rules(player, DELAY),
        link: { sendPacket: (bytes) => send(bytes.slice()), sendPlay: (message: PlayMessage) => send(message) },
        core,
        count: (label, onDone) => {
            counts.push(label);
            const timer = setTimeout(onDone, 3000);
            return () => clearTimeout(timer);
        },
    });
    return { play, core, counts, hidden: false, tickEvery: FRAME_MS, lastTick: 0 };
}

/** Plays `ms` of wall time: each visible page ticks at its own rate, as its animation frames come. */
async function run(ms: number) {
    const end = performance.now() + ms;
    while (performance.now() < end) {
        await vi.advanceTimersByTimeAsync(1);
        const t = performance.now();
        for (const s of [host, join]) {
            if (s.hidden || t - s.lastTick < s.tickEvery) continue;
            s.lastTick = t;
            s.play.tick();
        }
    }
}

beforeEach(() => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "Date", "performance"] });
    host = side(1, () => join);
    join = side(2, () => host);
    host.play.begin();
    join.play.begin();
});
afterEach(() => {
    vi.useRealTimers();
});

const NES_A = 0;
const NES_START = 3;
const NES_RIGHT = 7;

describe("frames and pads", () => {
    it("drives port 1 from the host's pad and port 2 from the joiner's, on both games", async () => {
        host.play.press(1, NES_A, true);
        join.play.press(1, NES_RIGHT, true);
        await run(500);
        expect(host.core.frames).toBeGreaterThan(20);
        const both = Math.min(host.core.frames, join.core.frames);
        expect(host.core.pads().slice(0, both)).toEqual(join.core.pads().slice(0, both));
        expect(host.core.pads()).toContainEqual([1 << NES_A, 1 << NES_RIGHT]);
    });

    it("runs the first frames on released pads, then each press the input delay later", async () => {
        host.play.press(1, NES_A, true);
        await run(200);
        expect(host.core.pads().slice(0, DELAY)).toEqual([
            [0, 0],
            [0, 0],
        ]);
        expect(host.core.pads()[DELAY]).toEqual([1 << NES_A, 0]);
    });

    it("lets every key and gamepad on this computer drive its one player", async () => {
        join.play.press(1, NES_A, true);
        join.play.press(2, NES_START, true);
        await run(100);
        expect(host.core.pads()[host.core.pads().length - 1]).toEqual([0, (1 << NES_A) | (1 << NES_START)]);
        join.play.press(2, NES_START, false);
        await run(100);
        expect(host.core.pads()[host.core.pads().length - 1]).toEqual([0, 1 << NES_A]);
    });

    it("does not run frames before it begins, nor after playing together has ended", async () => {
        const late = side(1, () => join);
        late.play.tick();
        expect(late.core.frames).toBe(0);
        await run(100);
        const ran = host.core.frames;
        host.play.ended("me");
        await run(100);
        expect(host.core.frames).toBe(ran);
        expect(host.play.view()).toMatchObject({ phase: "ended", ended: "me", waiting: false });
    });
});

describe("pausing", () => {
    it("stops both games after the same frame, and says who paused", async () => {
        await run(200);
        host.play.pause();
        await run(300);
        expect(host.play.view()).toMatchObject({ phase: "paused", pausedBy: "me", pauseReason: "pause" });
        expect(join.play.view()).toMatchObject({ phase: "paused", pausedBy: "them", pauseReason: "pause" });
        expect(host.core.frames).toBe(join.core.frames);
        expect(host.core.pads().flat().every((pad) => pad < 0x1000)).toBe(true);
    });

    it("is resumed by either player, both counting Resuming and then playing on", async () => {
        host.play.pause();
        await run(300);
        const paused = host.core.frames;
        join.play.pause();
        await run(50);
        expect(host.counts).toEqual(["resume"]);
        expect(join.counts).toEqual(["resume"]);
        expect(host.play.view()).toMatchObject({ phase: "counting", count: "resume" });
        await run(2900);
        expect(host.core.frames).toBe(paused);
        await run(500);
        expect(host.play.view().phase).toBe("playing");
        expect(join.play.view().phase).toBe("playing");
        expect(host.core.frames).toBeGreaterThan(paused);
    });

    it("is one pause when both press it at almost the same moment", async () => {
        await run(200);
        host.play.pause();
        await run(5);
        join.play.pause();
        await run(300);
        expect(host.play.view()).toMatchObject({ phase: "paused", pausedBy: "me" });
        expect(join.play.view()).toMatchObject({ phase: "paused", pausedBy: "them" });
        host.play.resume();
        await run(4000);
        expect(host.play.view().phase).toBe("playing");
        expect(join.play.view().phase).toBe("playing");
    });

    it("waits for a frame to carry a pause pressed while the other player's game is behind", async () => {
        await run(100);
        join.hidden = true;
        await run(600);
        host.play.pause();
        await run(300);
        expect(host.play.view().phase).toBe("playing");
        join.hidden = false;
        await run(300);
        expect(host.play.view()).toMatchObject({ phase: "paused", pausedBy: "me" });
        expect(join.play.view()).toMatchObject({ phase: "paused", pausedBy: "them" });
    });
});

describe("windows", () => {
    it("pause both, and the other side is told which window", async () => {
        for (const [window, reason] of [
            ["settings", "settings"],
            ["keys", "keys"],
            ["together", "together"],
        ] as const) {
            host.play.windowOpened(window);
            await run(300);
            expect(host.play.view()).toMatchObject({ phase: "paused", pausedBy: "me", pauseReason: reason });
            expect(join.play.view()).toMatchObject({ phase: "paused", pausedBy: "them", pauseReason: reason });
            host.play.windowClosed(window);
            await run(3500);
            expect(join.counts[join.counts.length - 1]).toBe("resume");
            expect(join.play.view().phase).toBe("playing");
        }
    });

    it("resume both when closed after a frame carried the pause but before this game reached it", async () => {
        await run(200);
        const before = join.core.frames;
        join.play.windowOpened("settings");
        // The next frame to run carries the pause, for a frame the input delay later.
        while (join.core.frames === before) await run(1);
        expect(join.play.view().phase).toBe("playing");
        join.play.windowClosed("settings");
        await run(300);
        expect(join.counts).toEqual(["resume"]);
        expect(host.counts).toEqual(["resume"]);
        await run(3500);
        expect(host.play.view().phase).toBe("playing");
        expect(join.play.view().phase).toBe("playing");
    });

    it("do not resume a game that was paused before they opened", async () => {
        join.play.pause();
        await run(300);
        host.play.windowOpened("keys");
        host.play.windowClosed("keys");
        await run(300);
        expect(host.play.view()).toMatchObject({ phase: "paused", pausedBy: "them" });
        expect(host.counts).toEqual([]);
    });

    it("let the game run on behind an open window once the other player resumes, and closing it then changes nothing", async () => {
        host.play.windowOpened("settings");
        await run(300);
        join.play.resume();
        await run(3500);
        expect(host.play.view().phase).toBe("playing");
        host.play.windowClosed("settings");
        await run(300);
        expect(host.counts).toEqual(["resume"]);
        expect(host.play.view().phase).toBe("playing");
    });
});

describe("resetting", () => {
    it("resets both games after the same frame, and both count saying who reset", async () => {
        await run(300);
        join.play.reset(true);
        await run(100);
        expect(host.core.resets()).toEqual(join.core.resets());
        expect(host.core.resets()).toEqual([{ reset: true, afterFrames: expect.any(Number) }]);
        expect(host.counts).toEqual(["reset-them"]);
        expect(join.counts).toEqual(["reset-me"]);
        expect(join.play.view()).toMatchObject({ phase: "counting", count: "reset-me" });
        await run(3500);
        expect(host.play.view().phase).toBe("playing");
        const both = Math.min(host.core.played.length, join.core.played.length);
        expect(host.core.played.slice(0, both)).toEqual(join.core.played.slice(0, both));
    });

    it("hard-resets both when the choice under the arrow is a hard reset", async () => {
        host.play.reset(false);
        await run(100);
        expect(join.core.resets()).toEqual([{ reset: false, afterFrames: expect.any(Number) }]);
    });

    it("resets both at the paused frame when the game is paused", async () => {
        host.play.pause();
        await run(300);
        join.play.reset(true);
        await run(100);
        expect(host.core.resets()).toEqual(join.core.resets());
        expect(host.core.resets()).toHaveLength(1);
        expect(host.counts).toEqual(["reset-them"]);
        await run(3500);
        expect(host.play.view().phase).toBe("playing");
        expect(join.play.view().phase).toBe("playing");
    });

    it("refuses a reset said for another frame than this game is at", async () => {
        host.play.pause();
        await run(300);
        host.play.message({ t: "reset", soft: true, at: host.core.frames + 5 });
        expect(host.core.resets()).toEqual([]);
        expect(host.play.view().phase).toBe("paused");
    });
});

describe("falling behind", () => {
    it("shows waiting once the other player's game has held this one half a second, and clears by itself", async () => {
        await run(200);
        join.hidden = true;
        await run(400);
        expect(host.play.view().waiting).toBe(false);
        await run(200);
        expect(host.play.view().waiting).toBe(true);
        expect(join.play.view().waiting).toBe(false);
        join.hidden = false;
        await run(200);
        expect(host.play.view()).toMatchObject({ phase: "playing", waiting: false });
        expect(host.counts).toEqual([]);
    });

    it("shows catching up on the game that stays behind, which runs two frames a tick meanwhile", async () => {
        await run(200);
        join.tickEvery = 50;
        await run(1000);
        expect(join.play.view().catchingUp).toBe(true);
        const before = join.core.frames;
        join.play.tick();
        expect(join.core.frames - before).toBeLessThanOrEqual(2);
        join.tickEvery = FRAME_MS;
        await run(500);
        expect(join.play.view().catchingUp).toBe(false);
    });

    it("shows neither on two games ticking together, a few milliseconds apart", async () => {
        join.lastTick = 7;
        const seen: boolean[] = [];
        for (const s of [host, join]) s.play.subscribe((v) => seen.push(v.waiting || v.catchingUp));
        await run(3000);
        expect(seen).not.toContain(true);
    });

    it("never shows waiting while paused or counting", async () => {
        host.play.pause();
        await run(300);
        join.hidden = true;
        await run(1000);
        expect(host.play.view().waiting).toBe(false);
    });
});
