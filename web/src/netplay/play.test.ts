import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createPlay, type CountLabel, type LockstepRules, type Play, type PlayCore } from "./play";
import type { PlayData, PlayMessage, ResyncState } from "./session";

// Playing together (nr-ayk5.5): two play controllers, the host's and the joiner's, over an in-memory
// link with a fixed one-way delay, each with a stand-in for the Rust lockstep that follows its
// rules (a frame runs only once both pads for it are known; the first `delay` frames run on
// released pads) and a stand-in core that records what it was asked to play.

const ONE_WAY_MS = 10;
const FRAME_MS = 1000 / 60;
const DELAY = 2;
/** The games are compared after every tenth frame. */
const HASH_EVERY = 10;

/** The lockstep rules, as `platform::netplay::lockstep` has them, without the wire format. */
class Rules implements LockstepRules {
    private local: number[];
    private remote: (number | undefined)[];
    private next = 0;
    private localHashes = new Map<number, number>();
    private remoteHashes = new Map<number, number>();
    private newestHash: [number, number] | null = null;
    private hashFloor = 0;
    /** Says the games differ, whatever the hashes: a test's way to make a mismatch at a moment of its choosing. */
    forceDesync = false;
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
        return new TextEncoder().encode(JSON.stringify({ pads: this.local, hash: this.newestHash }));
    }
    receive(bytes: Uint8Array): boolean {
        const got = JSON.parse(new TextDecoder().decode(bytes)) as number[] | { pads: number[]; hash: [number, number] | null };
        const pads = Array.isArray(got) ? got : got.pads;
        pads.forEach((pad, frame) => {
            this.remote[frame] ??= pad;
        });
        if (!Array.isArray(got) && got.hash && got.hash[0] >= this.hashFloor) this.remoteHashes.set(got.hash[0], got.hash[1]);
        return true;
    }
    record_state_hash(frame: number, hash: number) {
        if (frame % HASH_EVERY !== 0 || frame < this.hashFloor) return;
        this.localHashes.set(frame, hash);
        this.newestHash = [frame, hash];
    }
    desynced(): boolean {
        if (this.forceDesync) return true;
        for (const [frame, hash] of this.localHashes) {
            const theirs = this.remoteHashes.get(frame);
            if (theirs !== undefined && theirs !== hash) return true;
        }
        return false;
    }
    resynced() {
        this.forceDesync = false;
        this.localHashes.clear();
        this.remoteHashes.clear();
        this.newestHash = null;
        this.hashFloor = this.next;
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
    /** The game's whole state: it follows every frame's pads, so two games fed the same pads agree. */
    state = 1;
    loads: number[] = [];
    loadFails = false;
    runFrame(pad1: number, pad2: number) {
        this.played.push({ frame: this.frames++, pads: [pad1, pad2] });
        this.state = (Math.imul(this.state, 31) + pad1 * 7 + pad2 * 13 + 1) >>> 0;
    }
    stateHash() {
        return this.state;
    }
    saveState() {
        return new TextEncoder().encode(String(this.state));
    }
    loadState(bytes: Uint8Array) {
        if (this.loadFails) return false;
        this.state = Number(new TextDecoder().decode(bytes));
        this.loads.push(this.state);
        return true;
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
type Side = {
    play: Play;
    core: Core;
    rules: Rules;
    counts: CountLabel[];
    hidden: boolean;
    tickEvery: number;
    lastTick: number;
    /** Rewrites, or swallows (null), a resync state this side sends. */
    tamperState: (state: ResyncState) => ResyncState | null;
    gaveUp: number;
};

let host: Side;
let join: Side;

/** In order, `ONE_WAY_MS` later, as an ordered data channel delivers. */
function deliverer(to: () => Side) {
    let lastAt = 0;
    return (data: PlayData) => {
        const at = Math.max(performance.now() + ONE_WAY_MS, lastAt);
        lastAt = at;
        setTimeout(() => {
            to().play.hear(data);
        }, at - performance.now());
    };
}

function side(player: 1 | 2, other: () => Side): Side {
    const core = new Core();
    const rules = new Rules(player, DELAY);
    const counts: CountLabel[] = [];
    const send = deliverer(other);
    const me: Side = {
        play: undefined as unknown as Play,
        core,
        rules,
        counts,
        hidden: false,
        tickEvery: FRAME_MS,
        lastTick: 0,
        tamperState: (state) => state,
        gaveUp: 0,
    };
    me.play = createPlay({
        player,
        lockstep: rules,
        hashEvery: HASH_EVERY,
        link: {
            sendPacket: (bytes) => send(bytes.slice()),
            sendPlay: (message: PlayMessage) => send(message),
            sendState: (at, bytes) => {
                const out = me.tamperState({ t: "state", at, bytes: bytes.slice() });
                if (out) send(out);
            },
            // The session ends it on this side and tells the other, which ends it there.
            giveUp: () => {
                me.gaveUp++;
                setTimeout(() => other().play.ended("unsynced"), ONE_WAY_MS);
            },
        },
        core,
        count: (label, onDone) => {
            counts.push(label);
            const timer = setTimeout(onDone, 3000);
            return () => clearTimeout(timer);
        },
    });
    return me;
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

/** One side alone, fed the other's pads by hand; what it sends is kept. */
function alone(player: 1 | 2) {
    const core = new Core();
    const counts: CountLabel[] = [];
    const sent: PlayMessage[] = [];
    const play = createPlay({
        player,
        lockstep: new Rules(player, DELAY),
        hashEvery: HASH_EVERY,
        link: { sendPacket: () => {}, sendPlay: (message) => sent.push(message), sendState: () => {}, giveUp: () => {} },
        core,
        count: (label, onDone) => {
            counts.push(label);
            const timer = setTimeout(onDone, 3000);
            return () => clearTimeout(timer);
        },
    });
    play.begin();
    return { play, core, counts, sent };
}
/** The other player's pads for frames 0 on, with a pause carried by frame 4: both stop after it. */
const PAUSED_AFTER_4 = [0, 0, 0, 0, 1 << 12, 0, 0, 0];
const pads = (list: number[]) => new TextEncoder().encode(JSON.stringify(list));

// Review of PR #3390: what reaches a game that is still on its way to the frame both stop at, and
// presses made close to a pause, a count or another press.
describe("messages for a frame this game has not reached yet", () => {
    it("plays a reset there once it arrives, so both games reset after the same frame", () => {
        const s = alone(2);
        s.play.receive(pads(PAUSED_AFTER_4));
        s.play.tick();
        expect(s.core.frames).toBeLessThan(5);
        s.play.message({ t: "reset", soft: true, at: 5 });
        expect(s.core.resets()).toEqual([]);
        for (let i = 0; i < 5; i++) s.play.tick();
        expect(s.core.resets()).toEqual([{ reset: true, afterFrames: 5 }]);
        expect(s.counts).toEqual(["reset-them"]);
    });

    it("resumes there once it arrives, rather than staying paused", () => {
        const s = alone(2);
        s.play.receive(pads(PAUSED_AFTER_4));
        s.play.tick();
        s.play.message({ t: "resume", at: 5 });
        for (let i = 0; i < 5; i++) s.play.tick();
        expect(s.counts).toEqual(["resume"]);
        expect(s.play.view()).toMatchObject({ phase: "counting", count: "resume" });
    });

    it("says which frame its own resume and reset are for", () => {
        const s = alone(2);
        s.play.receive(pads(PAUSED_AFTER_4));
        for (let i = 0; i < 5; i++) s.play.tick();
        expect(s.play.view().phase).toBe("paused");
        s.play.resume();
        expect(s.sent).toEqual([{ t: "resume", at: 5 }]);
    });

    it("drops a message for a frame this game has already passed", () => {
        const s = alone(2);
        s.play.receive(pads(PAUSED_AFTER_4));
        for (let i = 0; i < 5; i++) s.play.tick();
        s.play.message({ t: "resume", at: 3 });
        s.play.message({ t: "reset", soft: true, at: 3 });
        expect(s.play.view().phase).toBe("paused");
        expect(s.core.resets()).toEqual([]);
    });
});

describe("presses close together", () => {
    /** Pauses and resumes, calling `then` on the joiner the moment its Resuming count is over. */
    async function pauseAndResume(then: () => void) {
        await run(200);
        host.play.pause();
        await run(300);
        let done = false;
        join.play.subscribe((v) => {
            if (!done && v.phase === "playing") {
                done = true;
                then();
            }
        });
        host.play.resume();
        await run(3100);
        expect(done).toBe(true);
    }

    it("takes a pause pressed straight after a resume count as a new pause", async () => {
        await pauseAndResume(() => join.play.pause());
        await run(300);
        expect(host.play.view()).toMatchObject({ phase: "paused", pausedBy: "them" });
        expect(join.play.view()).toMatchObject({ phase: "paused", pausedBy: "me" });
    });

    it("pauses for a window opened straight after a count, and again the next time it opens", async () => {
        await pauseAndResume(() => join.play.windowOpened("settings"));
        await run(300);
        expect(host.play.view()).toMatchObject({ phase: "paused", pauseReason: "settings" });
        join.play.windowClosed("settings");
        await run(3500);
        join.play.windowOpened("settings");
        await run(300);
        expect(host.play.view()).toMatchObject({ phase: "paused", pauseReason: "settings" });
        expect(join.play.view()).toMatchObject({ phase: "paused", pauseReason: "settings" });
    });

    it("pauses for a window opened during a count, once the count is over", async () => {
        await run(200);
        host.play.pause();
        await run(300);
        host.play.resume();
        await run(500);
        expect(host.play.view().phase).toBe("counting");
        join.play.windowOpened("keys");
        await run(3000);
        expect(host.play.view()).toMatchObject({ phase: "paused", pausedBy: "them", pauseReason: "keys" });
        expect(join.play.view()).toMatchObject({ phase: "paused", pausedBy: "me", pauseReason: "keys" });
    });

    it("resets both games once when both press Reset at almost the same moment", async () => {
        await run(200);
        host.play.reset(true);
        await run(20);
        join.play.reset(true);
        await run(4000);
        expect(host.core.resets()).toHaveLength(1);
        expect(join.core.resets()).toHaveLength(1);
        expect(host.counts).toHaveLength(1);
        expect(join.counts).toHaveLength(1);
    });
});

// Second review of PR #3390: what a reset or a count does to a window's pause, and a message that
// arrives while this game still counts.
describe("a window, a count and a reset together", () => {
    const P = 1 << 12;
    const RESET = 5 << 12;
    /** Ticks until the phase is `phase`, at most `limit` times. */
    function tickUntil(s: ReturnType<typeof alone>, phase: string, limit = 20) {
        for (let i = 0; i < limit && s.play.view().phase !== phase; i++) s.play.tick();
        expect(s.play.view().phase).toBe(phase);
    }

    it("holds a message that arrives while this game still counts, and plays it at the frame it is for", () => {
        const s = alone(2);
        s.play.receive(pads([0, 0, RESET, 0, P, 0, 0, 0]));
        tickUntil(s, "counting");
        s.play.message({ t: "reset", soft: false, at: 5 });
        vi.advanceTimersByTime(3000);
        tickUntil(s, "counting");
        expect(s.core.resets()).toEqual([
            { reset: true, afterFrames: 3 },
            { reset: false, afterFrames: 5 },
        ]);
    });

    it("still resumes after a window closed before its pause, when another player's repeated press is dropped first", () => {
        const s = alone(2);
        s.play.receive(pads([0, 0, P, 0, P]));
        tickUntil(s, "paused");
        s.play.resume();
        vi.advanceTimersByTime(3000);
        expect(s.play.view().phase).toBe("playing");
        s.play.windowOpened("settings");
        s.play.tick();
        s.play.windowClosed("settings");
        s.play.tick();
        s.play.receive(pads([0, 0, P, 0, P, 0, 0, 0, 0, 0, 0]));
        tickUntil(s, "counting");
        expect(s.sent.filter((m) => m.t === "resume")).toHaveLength(2);
    });

    it("resumes on closing a window whose pause was waiting while a reset was played", () => {
        const s = alone(2);
        s.play.receive(pads([0, 0, P, 0, 0, 0, 0, 0, 0, 0]));
        tickUntil(s, "paused");
        s.play.resume();
        s.play.windowOpened("keys");
        s.play.message({ t: "reset", soft: true, at: 3 });
        expect(s.counts).toEqual(["resume", "reset-them"]);
        vi.advanceTimersByTime(3000);
        tickUntil(s, "paused");
        expect(s.play.view()).toMatchObject({ pausedBy: "me", pauseReason: "keys" });
        s.play.windowClosed("keys");
        expect(s.play.view()).toMatchObject({ phase: "counting", count: "resume" });
    });

    it("does not pause for a window opened and closed during a count", async () => {
        await run(200);
        host.play.pause();
        await run(300);
        host.play.resume();
        await run(500);
        host.play.windowOpened("keys");
        host.play.windowClosed("keys");
        await run(3500);
        expect(host.play.view().phase).toBe("playing");
        expect(join.play.view().phase).toBe("playing");
    });
});

// Resyncing (nr-ayk5.6): the games are compared as they run; when they differ both stop after the
// same frame, the joiner's game is set to the host's, and both count "Resynced" and play on.
describe("resyncing", () => {
    /** Each side's frame count whenever its phase turns to `phase`. */
    function phaseFrames(s: Side, phase: string): number[] {
        const frames: number[] = [];
        s.play.subscribe((view) => {
            if (view.phase === phase && frames.length === resyncs(s)) frames.push(s.core.frames);
        });
        return frames;
    }
    const resyncs = (s: Side) => s.counts.filter((c) => c === "resync").length;
    const dropStates = () => {
        host.tamperState = () => null;
    };

    it("plays on unseen while the two games match", async () => {
        await run(2000);
        expect(host.play.view().phase).toBe("playing");
        expect(join.play.view().phase).toBe("playing");
        expect(host.core.loads).toEqual([]);
    });

    it("stops both games after the same frame, sets the joiner's to the host's, and both count Resynced then play on", async () => {
        const hostStopped = phaseFrames(host, "resyncing");
        const joinStopped = phaseFrames(join, "resyncing");
        await run(300);
        join.core.state ^= 0x55;
        await run(1000);
        expect(hostStopped).toHaveLength(1);
        expect(hostStopped).toEqual(joinStopped);
        expect(host.counts).toEqual(["resync"]);
        expect(join.counts).toEqual(["resync"]);
        expect(join.core.loads).toHaveLength(1);
        expect(host.core.loads).toEqual([]);
        await run(3500);
        expect(host.play.view().phase).toBe("playing");
        expect(join.play.view().phase).toBe("playing");
        await run(1000);
        // Both have run the same frames since: the same pads on the same state.
        const both = Math.min(host.core.frames, join.core.frames);
        expect(host.core.pads().slice(0, both)).toEqual(join.core.pads().slice(0, both));
        expect(host.core.frames - join.core.frames).toBeLessThanOrEqual(DELAY + 1);
        expect(host.rules.desynced() || join.rules.desynced()).toBe(false);
    });

    it("is one resync when both games notice the same difference", async () => {
        await run(300);
        join.core.state ^= 0x55;
        await run(6000);
        expect(resyncs(host)).toBe(1);
        expect(resyncs(join)).toBe(1);
        expect(join.core.loads).toHaveLength(1);
    });

    it("looks and works the same every time, however often it happens", async () => {
        for (let round = 1; round <= 3; round++) {
            join.core.state ^= round;
            await run(5000);
            expect(resyncs(host)).toBe(round);
            expect(resyncs(join)).toBe(round);
            expect(host.play.view().phase).toBe("playing");
        }
        expect(join.core.loads).toHaveLength(3);
    });

    it("starts no resync while the game is paused, only once it runs again", async () => {
        await run(200);
        host.play.pause();
        await run(300);
        host.rules.forceDesync = true;
        await run(2000);
        expect(host.play.view().phase).toBe("paused");
        expect(resyncs(host)).toBe(0);
        host.play.resume();
        await run(3000);
        expect(host.counts).toEqual(["resume"]);
        await run(1000);
        expect(resyncs(host)).toBe(1);
        expect(resyncs(join)).toBe(1);
    });

    it("ignores Pause, Reset and Resume while resyncing and while counting", async () => {
        await run(200);
        dropStates();
        join.core.state ^= 0x55;
        await run(1000);
        expect(host.play.view().phase).toBe("resyncing");
        expect(join.play.view().phase).toBe("resyncing");
        for (const s of [host, join]) {
            s.play.pause();
            s.play.resume();
            s.play.reset(true);
            s.play.reset(false);
        }
        await run(500);
        expect(host.play.view().phase).toBe("resyncing");
        expect(join.play.view().phase).toBe("resyncing");
        host.tamperState = (state) => state;
        join.core.state ^= 0x55;
        await run(100);
        expect(host.core.resets()).toEqual([]);
    });

    it("ignores Pause and Reset during the Resynced count", async () => {
        await run(200);
        join.core.state ^= 0x55;
        await run(1000);
        expect(host.play.view()).toMatchObject({ phase: "counting", count: "resync" });
        host.play.pause();
        host.play.reset(true);
        join.play.pause();
        join.play.reset(false);
        await run(3500);
        expect(host.play.view().phase).toBe("playing");
        expect(join.play.view().phase).toBe("playing");
        expect(host.core.resets()).toEqual([]);
        expect(join.core.resets()).toEqual([]);
    });

    it("carries out a Pause pressed before the resync stopped the games, once the count is over", async () => {
        await run(200);
        host.rules.forceDesync = true;
        host.play.pause();
        await run(5000);
        expect(resyncs(host)).toBe(1);
        expect(host.play.view()).toMatchObject({ phase: "paused", pausedBy: "me", pauseReason: "pause" });
        expect(join.play.view()).toMatchObject({ phase: "paused", pausedBy: "them", pauseReason: "pause" });
    });

    it("keeps a Pause carried by the very frame the other player's resync stopped both games on", async () => {
        await run(200);
        host.rules.forceDesync = true;
        // The host's next frame records the resync; the joiner's records its pause for the same frame.
        join.play.pause();
        await run(5000);
        expect(resyncs(join)).toBe(1);
        expect(join.play.view()).toMatchObject({ phase: "paused", pausedBy: "me", pauseReason: "pause" });
    });

    it("pauses for a window opened during a resync, once the count is over", async () => {
        await run(200);
        join.core.state ^= 0x55;
        await run(500);
        expect(join.play.view().phase).not.toBe("playing");
        join.play.windowOpened("settings");
        await run(4000);
        expect(host.play.view()).toMatchObject({ phase: "paused", pausedBy: "them", pauseReason: "settings" });
        expect(join.play.view()).toMatchObject({ phase: "paused", pausedBy: "me", pauseReason: "settings" });
    });

    it("ends playing together on both when the games are not back in step 15 seconds after it began", async () => {
        await run(200);
        dropStates();
        join.core.state ^= 0x55;
        await run(1000);
        expect(host.play.view().phase).toBe("resyncing");
        await run(13_000);
        expect(host.play.view().phase).toBe("resyncing");
        expect(join.play.view().phase).toBe("resyncing");
        await run(2000);
        expect(host.play.view()).toMatchObject({ phase: "ended", ended: "unsynced" });
        expect(join.play.view()).toMatchObject({ phase: "ended", ended: "unsynced" });
        expect(host.gaveUp + join.gaveUp).toBeGreaterThanOrEqual(1);
    });

    it("ends playing together on both when the joiner cannot load the host's game", async () => {
        await run(200);
        join.core.loadFails = true;
        join.core.state ^= 0x55;
        await run(1000);
        expect(join.play.view()).toMatchObject({ phase: "ended", ended: "unsynced" });
        expect(host.play.view()).toMatchObject({ phase: "ended", ended: "unsynced" });
        expect(join.gaveUp).toBe(1);
    });

    it("ends playing together on both when the host's game did not arrive whole", async () => {
        await run(200);
        host.tamperState = (state) => ({ ...state, bytes: null });
        join.core.state ^= 0x55;
        await run(1000);
        expect(join.play.view()).toMatchObject({ phase: "ended", ended: "unsynced" });
        expect(host.play.view()).toMatchObject({ phase: "ended", ended: "unsynced" });
    });

    it("says the other player left when the link is lost during a resync, and gives up nothing later", async () => {
        await run(200);
        dropStates();
        join.core.state ^= 0x55;
        await run(1000);
        host.play.ended("left");
        join.play.ended("left");
        await run(20_000);
        expect(host.play.view()).toMatchObject({ phase: "ended", ended: "left" });
        expect(join.play.view()).toMatchObject({ phase: "ended", ended: "left" });
        expect(host.gaveUp + join.gaveUp).toBe(0);
    });
});
