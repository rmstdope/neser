import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { encodeCode } from "./code";
import { MAX_PACKAGE_BYTES, type GamePackage } from "./game_package";
import type { HostStart, JoinStart, PeerLink, PeerLinks } from "./peer_link";
import { CHUNK_BYTES, createNetplaySession, STALL_MS, WINDOW_BYTES, type NetplaySession, type NetplayStart } from "./session";

// Sending the game (nr-ayk5.4): a real host session and a real joiner session, connected over an
// in-memory pair of links with a fixed one-way delay.

const BUILD = { version: "1.3.0", commit: "4f2a9c1" };
const ONE_WAY_MS = 10;
const TITLE = "Super Mario Bros. 3";

type Data = string | Uint8Array;

/** One end of the pair; what it sends reaches the other end `ONE_WAY_MS` later, unless held or dropped. */
class End implements PeerLink {
    other!: End;
    listener: (data: Data) => void = () => {};
    closedListener: () => void = () => {};
    /** Rewrites, or swallows (null), what this end sends. */
    tamper: (data: Data) => Data | null = (data) => data;
    held = false;
    delay = ONE_WAY_MS;
    lastAt = 0;
    sentBytes = 0;
    sent: Data[] = [];

    send(data: Data) {
        this.sent.push(data);
        if (typeof data !== "string") this.sentBytes += data.length - 4;
        const out = this.tamper(typeof data === "string" ? data : data.slice());
        if (out === null || this.held) return;
        // An ordered channel: a message never overtakes one sent before it.
        const at = Math.max(performance.now() + this.delay, this.lastAt);
        this.lastAt = at;
        setTimeout(() => this.other.listener(out), at - performance.now());
    }
    onMessage(listener: (data: Data) => void) {
        this.listener = listener;
    }
    onClosed(listener: () => void) {
        this.closedListener = listener;
    }
    close() {}
    messages(t: string) {
        return this.sent.filter((d): d is string => typeof d === "string").map((d) => JSON.parse(d)).filter((m) => m.t === t);
    }
    /** A message that did not come from the session at this end: forged, or from a broken peer. */
    forge(data: Data) {
        setTimeout(() => this.other.listener(data), ONE_WAY_MS);
    }
}

function linksTo(end: End): PeerLinks {
    return {
        startHost: async (): Promise<HostStart> => ({ sdp: "offer", accept: async () => end, cancel: () => {} }),
        startJoin: async (): Promise<JoinStart> => ({ sdp: "answer", link: Promise.resolve(end), cancel: () => {} }),
    };
}

async function settle() {
    for (let i = 0; i < 50; i++) await new Promise((r) => setImmediate(r));
}

function game(size: number): GamePackage {
    const rom = new Uint8Array(size);
    for (let i = 0; i < size; i++) rom[i] = (i * 31) & 0xff;
    return { kind: "nes", name: "smb3.nes", rom, firmware: null, state: new Uint8Array([1, 2, 3]) };
}

let host: NetplaySession;
let join: NetplaySession;
let hostEnd: End;
let joinEnd: End;
let starts: { side: "host" | "join"; at: number; start: NetplayStart }[];
/** The joiner's check: how long loading the game took it, in ms, or null when it cannot run it. */
let checkGame: (game: GamePackage) => Promise<number | null>;
let confirmStart: (game: GamePackage) => boolean;

async function connected() {
    hostEnd = new End();
    joinEnd = new End();
    hostEnd.other = joinEnd;
    joinEnd.other = hostEnd;
    starts = [];
    checkGame = async () => 0;
    confirmStart = () => true;
    host = createNetplaySession({
        links: linksTo(hostEnd),
        build: BUILD,
        confirmStart: (g) => confirmStart(g),
        onStart: (start) => starts.push({ side: "host", at: performance.now(), start }),
    });
    join = createNetplaySession({
        links: linksTo(joinEnd),
        build: BUILD,
        checkGame: (g) => checkGame(g),
        onStart: (start) => starts.push({ side: "join", at: performance.now(), start }),
    });
    join.join();
    await join.paste(await encodeCode("host", BUILD, "offer"));
    join.next();
    host.host();
    for (let i = 0; i < 10_000 && host.view().ownCode === null; i++) await new Promise((r) => setImmediate(r));
    host.next();
    await host.paste(await encodeCode("reply", BUILD, "answer"));
    host.connect();
    await settle();
    await vi.advanceTimersByTimeAsync(2000);
    expect(host.view().phase).toBe("connected");
    expect(join.view().phase).toBe("connected");
}

/** Moves the clock in small steps, letting the async checks run between them. */
async function run(ms: number) {
    for (let t = 0; t < ms; t += ONE_WAY_MS) {
        await vi.advanceTimersByTimeAsync(ONE_WAY_MS);
        await settle();
    }
}

beforeEach(async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date", "performance"] });
    await connected();
});
afterEach(() => {
    host.cancel();
    join.cancel();
    vi.useRealTimers();
});

describe("sending the game", () => {
    it("shows the same progress on both sides, counting up to the whole game", async () => {
        host.startGame(game(2_000_000), TITLE);
        expect(host.view().sending).toMatchObject({ direction: "send", name: TITLE, done: 0 });
        const total = host.view().sending!.total;
        await run(30);
        expect(join.view().sending).toMatchObject({ direction: "receive", name: TITLE, total });
        const seen: number[] = [];
        for (let i = 0; i < 200 && host.view().sending; i++) {
            seen.push(host.view().sending!.done);
            await run(ONE_WAY_MS);
        }
        expect(seen.some((done) => done > 0 && done < total)).toBe(true);
        expect([...seen].sort((a, b) => a - b)).toEqual(seen);
        expect(host.view().sending).toBeNull();
        expect(join.view().sending).toBeNull();
    });

    it("hands both sides the host's game, the host half a round trip after telling the joiner, so both start together", async () => {
        const sent = game(100_000);
        host.startGame(sent, TITLE);
        await run(2000);
        expect(starts.map((s) => s.side).sort()).toEqual(["host", "join"]);
        const hostStart = starts.find((s) => s.side === "host")!;
        const joinStart = starts.find((s) => s.side === "join")!;
        expect(Math.abs(hostStart.at - joinStart.at)).toBeLessThanOrEqual(ONE_WAY_MS);
        expect(hostStart.start).toMatchObject({ role: "host", game: sent });
        expect(joinStart.start.role).toBe("join");
        expect(joinStart.start.game).toEqual(sent);
        expect(host.view()).toMatchObject({ phase: "connected", sending: null, notice: null });
    });

    it("never has more than the window out unacknowledged", async () => {
        joinEnd.held = true;
        host.startGame(game(2_000_000), TITLE);
        await run(500);
        expect(hostEnd.sentBytes).toBeGreaterThan(0);
        expect(hostEnd.sentBytes).toBeLessThanOrEqual(WINDOW_BYTES + CHUNK_BYTES);
    });

    it("is told to the joiner when the host cancels, and both are back at the connected step", async () => {
        host.startGame(game(2_000_000), TITLE);
        await run(100);
        host.cancelSending();
        expect(host.view()).toMatchObject({ phase: "connected", sending: null, notice: null });
        await run(100);
        expect(join.view()).toMatchObject({ phase: "connected", sending: null, notice: "host-stopped" });
        expect(starts).toEqual([]);
    });

    it("is told to the host when the joiner cancels", async () => {
        host.startGame(game(2_000_000), TITLE);
        await run(100);
        join.cancelSending();
        expect(join.view()).toMatchObject({ phase: "connected", sending: null, notice: null });
        await run(100);
        expect(host.view()).toMatchObject({ phase: "connected", sending: null, notice: "joiner-stopped" });
        await run(2000);
        expect(starts).toEqual([]);
    });

    it("did not arrive, on both sides, when a chunk was damaged on the way", async () => {
        let flipped = false;
        hostEnd.tamper = (data) => {
            if (typeof data === "string" || flipped || data.length < 100) return data;
            flipped = true;
            data[50] ^= 0xff;
            return data;
        };
        host.startGame(game(100_000), TITLE);
        await run(2000);
        expect(host.view()).toMatchObject({ phase: "connected", sending: null, notice: "not-arrived" });
        expect(join.view()).toMatchObject({ phase: "connected", sending: null, notice: "not-arrived" });
        expect(starts).toEqual([]);
    });

    it("did not arrive when the joiner's browser cannot load what came", async () => {
        checkGame = async () => null;
        host.startGame(game(50_000), TITLE);
        await run(2000);
        expect(host.view().notice).toBe("not-arrived");
        expect(join.view().notice).toBe("not-arrived");
        expect(starts).toEqual([]);
    });

    it("did not arrive when the game is announced as nothing, or as larger than any game", async () => {
        for (const total of [0, MAX_PACKAGE_BYTES + 1, -5, 1.5, "big"]) {
            hostEnd.forge(JSON.stringify({ t: "game", id: 99, name: TITLE, total, crc: 0 }));
            await run(30);
            expect(join.view()).toMatchObject({ sending: null, notice: "not-arrived" });
            expect(joinEnd.messages("broken").slice(-1)[0]).toEqual({ t: "broken", id: 99 });
        }
    });

    it("did not arrive when more came than was announced", async () => {
        hostEnd.forge(JSON.stringify({ t: "game", id: 7, name: TITLE, total: 10, crc: 0 }));
        const frame = new Uint8Array(4 + 11);
        new DataView(frame.buffer).setUint32(0, 7, true);
        hostEnd.forge(frame);
        await run(30);
        expect(join.view()).toMatchObject({ sending: null, notice: "not-arrived" });
    });

    it("ignores bytes it was not expecting, and messages it does not understand", async () => {
        hostEnd.forge(new Uint8Array([1, 2]));
        hostEnd.forge(new Uint8Array([9, 0, 0, 0, 5, 5]));
        hostEnd.forge("{not json");
        hostEnd.forge(JSON.stringify({ t: "start", id: 3 }));
        hostEnd.forge(JSON.stringify({ t: "got", id: "x", bytes: {} }));
        joinEnd.forge(JSON.stringify({ t: "arrived", id: 3 }));
        await run(30);
        expect(join.view()).toMatchObject({ phase: "connected", sending: null, notice: null });
        expect(host.view()).toMatchObject({ phase: "connected", sending: null, notice: null });
        expect(starts).toEqual([]);
    });

    it("did not arrive, on both sides, when nothing gets through for a while", async () => {
        host.startGame(game(2_000_000), TITLE);
        await run(100);
        hostEnd.held = true;
        joinEnd.held = true;
        await run(STALL_MS - 500);
        expect(host.view().sending).not.toBeNull();
        await run(1000);
        expect(host.view()).toMatchObject({ phase: "connected", sending: null, notice: "not-arrived" });
        expect(join.view()).toMatchObject({ phase: "connected", sending: null, notice: "not-arrived" });
    });

    it("ends as the other player disconnected when the link drops mid-send", async () => {
        host.startGame(game(2_000_000), TITLE);
        await run(100);
        hostEnd.closedListener();
        joinEnd.closedListener();
        expect(host.view()).toMatchObject({ phase: "failed", failure: "peer-left", sending: null });
        expect(join.view()).toMatchObject({ phase: "failed", failure: "peer-left", sending: null });
        expect(starts).toEqual([]);
    });

    it("can be tried again after it did not arrive, and clears the notice when it starts", async () => {
        checkGame = async () => null;
        host.startGame(game(20_000), TITLE);
        await run(1000);
        expect(host.view().notice).toBe("not-arrived");
        checkGame = async () => 0;
        host.startGame(game(20_000), TITLE);
        expect(host.view().notice).toBeNull();
        await run(2000);
        expect(starts.map((s) => s.side).sort()).toEqual(["host", "join"]);
    });

    it("cannot be cancelled by the joiner once the whole game has arrived, so the host does not start alone", async () => {
        joinEnd.tamper = (data) => {
            if (typeof data === "string" && JSON.parse(data).t === "arrived") join.cancelSending();
            return data;
        };
        host.startGame(game(50_000), TITLE);
        await run(2000);
        expect(starts.map((s) => s.side).sort()).toEqual(["host", "join"]);
    });

    it("cannot be cancelled by the host once it has told the joiner to start", async () => {
        hostEnd.tamper = (data) => {
            if (typeof data === "string" && JSON.parse(data).t === "start") host.cancelSending();
            return data;
        };
        host.startGame(game(50_000), TITLE);
        await run(2000);
        expect(starts.map((s) => s.side).sort()).toEqual(["host", "join"]);
    });

    it("starts neither side when the host cancels just as the last of it arrives", async () => {
        joinEnd.tamper = (data) => {
            if (typeof data === "string" && JSON.parse(data).t === "arrived") host.cancelSending();
            return data;
        };
        host.startGame(game(50_000), TITLE);
        await run(2000);
        expect(starts).toEqual([]);
        expect(host.view()).toMatchObject({ sending: null, notice: null });
        expect(join.view()).toMatchObject({ sending: null, notice: "host-stopped" });
    });

    it("waits on the host's side as long as setting the game up takes the joiner, so both counts begin together", async () => {
        checkGame = async () => 300;
        host.startGame(game(50_000), TITLE);
        await run(3000);
        const hostStart = starts.find((s) => s.side === "host")!;
        const joinStart = starts.find((s) => s.side === "join")!;
        // The joiner spends those 300 ms loading the game before its count begins.
        expect(Math.abs(hostStart.at - (joinStart.at + 300))).toBeLessThanOrEqual(ONE_WAY_MS);
    });

    it("waits no longer than two seconds for a joiner that claims a slow set-up", async () => {
        checkGame = async () => 60_000;
        host.startGame(game(50_000), TITLE);
        await run(4000);
        const hostStart = starts.find((s) => s.side === "host")!;
        const joinStart = starts.find((s) => s.side === "join")!;
        expect(hostStart.at - joinStart.at).toBeLessThanOrEqual(2000 + ONE_WAY_MS);
    });

    it("times the host's start from the response time before sending, not one slowed by the game queued on the link", async () => {
        await run(1000);
        expect(host.view().rtt).toBe(2 * ONE_WAY_MS);
        // The game queued on the link makes every round trip during the send long; once it has
        // all arrived the link is quick again.
        hostEnd.delay = joinEnd.delay = 150;
        joinEnd.tamper = (data) => {
            if (typeof data === "string" && JSON.parse(data).t === "arrived") hostEnd.delay = joinEnd.delay = ONE_WAY_MS;
            return data;
        };
        host.startGame(game(2_000_000), TITLE);
        await run(12_000);
        expect(starts.map((s) => s.side).sort()).toEqual(["host", "join"]);
        const [a, b] = starts;
        expect(Math.abs(a.at - b.at)).toBeLessThanOrEqual(ONE_WAY_MS);
    });

    it("starts neither side when the host no longer runs the game it sent", async () => {
        confirmStart = () => false;
        host.startGame(game(50_000), TITLE);
        await run(2000);
        expect(starts).toEqual([]);
        expect(host.view()).toMatchObject({ sending: null, notice: "not-arrived" });
        expect(join.view()).toMatchObject({ sending: null, notice: "not-arrived" });
    });

    it("refuses a cancel on either side once the whole game has arrived", async () => {
        let hostTried: boolean | null = null;
        let joinTried: boolean | null = null;
        joinEnd.tamper = (data) => {
            if (typeof data === "string" && JSON.parse(data).t === "arrived") joinTried = join.cancelSending();
            return data;
        };
        hostEnd.tamper = (data) => {
            if (typeof data === "string" && JSON.parse(data).t === "start") hostTried = host.cancelSending();
            return data;
        };
        host.startGame(game(50_000), TITLE);
        await run(2000);
        expect(joinTried).toBe(false);
        expect(hostTried).toBe(false);
    });

    it("tells the host whether a cancel happened", async () => {
        host.startGame(game(2_000_000), TITLE);
        await run(100);
        expect(host.cancelSending()).toBe(true);
        expect(host.cancelSending()).toBe(false);
    });

    it("keeps measuring the response time while sending", async () => {
        const pongsBefore = hostEnd.messages("ping").length;
        host.startGame(game(2_000_000), TITLE);
        await run(3000);
        expect(hostEnd.messages("ping").length).toBeGreaterThan(pongsBefore + 1);
        expect(host.view().rtt).toBe(2 * ONE_WAY_MS);
    });

    it("is only the host's to start, and only once connected", async () => {
        join.startGame(game(100), TITLE);
        expect(join.view().sending).toBeNull();
        host.disconnect();
        host.startGame(game(100), TITLE);
        expect(host.view().sending).toBeNull();
    });
});
