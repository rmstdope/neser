import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { encodeCode } from "./code";
import type { HostStart, JoinStart, PeerLink, PeerLinks } from "./peer_link";
import { createNetplaySession, type NetplaySession } from "./session";

const OURS = { version: "1.3.0", commit: "4f2a9c1" };
const THEIRS = { version: "1.3.0", commit: "b81e07d" };

/** The other browser at the end of a fake link: it answers pings after `rtt` ms, and logs the rest. */
class FakeLink implements PeerLink {
    sent: Record<string, unknown>[] = [];
    closed = false;
    rtt: number;
    private messageListener: (text: string) => void = () => {};
    private closedListener: () => void = () => {};

    constructor(rtt: number) {
        this.rtt = rtt;
    }

    send(text: string) {
        const message = JSON.parse(text);
        this.sent.push(message);
        if (message.t === "ping") setTimeout(() => this.deliver({ t: "pong", id: message.id, at: message.at }), this.rtt);
    }
    onMessage(listener: (text: string) => void) {
        this.messageListener = listener;
    }
    onClosed(listener: () => void) {
        this.closedListener = listener;
    }
    close() {
        this.closed = true;
    }
    deliver(message: Record<string, unknown>) {
        this.messageListener(JSON.stringify(message));
    }
    /** The other side went away: its page closed or the network dropped. */
    drop() {
        this.closedListener();
    }
    sentOfType(t: string) {
        return this.sent.filter((m) => m.t === t);
    }
}

function deferred<T>() {
    let resolve!: (value: T) => void;
    const promise = new Promise<T>((r) => (resolve = r));
    return { promise, resolve };
}

function fakeLinks() {
    const hostCancel = vi.fn();
    const joinCancel = vi.fn();
    const accepted = deferred<PeerLink>();
    const joined = deferred<PeerLink>();
    let offers = 0;
    const links: PeerLinks = {
        startHost: async (): Promise<HostStart> => ({
            sdp: `offer-sdp-${++offers}`,
            accept: vi.fn(() => accepted.promise),
            cancel: hostCancel,
        }),
        startJoin: async (hostSdp: string): Promise<JoinStart> => ({
            sdp: `answer-to-${hostSdp}`,
            link: joined.promise,
            cancel: joinCancel,
        }),
    };
    return { links, accepted, joined, hostCancel, joinCancel };
}

/**
 * Lets promises, compression streams and zero-delay work finish without moving the clock. A code
 * on its way (a step that shows one, not yet failed) is waited for, however many turns the
 * compression stream takes on a slow machine: a fixed count of turns was enough here and not on CI.
 */
async function settle() {
    const turn = () => new Promise((r) => setImmediate(r));
    for (let i = 0; i < 20; i++) await turn();
    const codeDue = () => {
        const view = session?.view();
        if (!view || view.phase === "failed" || view.ownCode !== null) return false;
        return (view.role === "host" && view.step === 1) || (view.role === "join" && view.step === 2);
    };
    for (let i = 0; i < 10_000 && codeDue(); i++) await turn();
}

let session: NetplaySession;

beforeEach(() => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date", "performance"] });
});
afterEach(() => {
    session?.cancel();
    vi.useRealTimers();
});

async function hostToConnect(rtt: number) {
    const fake = fakeLinks();
    session = createNetplaySession({ links: fake.links, build: OURS });
    session.host();
    await settle();
    session.next();
    await session.paste(await encodeCode("reply", OURS, "answer-sdp"));
    session.connect();
    const link = new FakeLink(rtt);
    fake.accepted.resolve(link);
    await settle();
    return { fake, link };
}

async function joinToWaiting() {
    const fake = fakeLinks();
    session = createNetplaySession({ links: fake.links, build: OURS });
    session.join();
    await session.paste(await encodeCode("host", OURS, "offer-sdp"));
    session.next();
    await settle();
    return fake;
}

describe("hosting", () => {
    it("starts at step 1 with the host's code to send", async () => {
        const fake = fakeLinks();
        session = createNetplaySession({ links: fake.links, build: OURS });
        session.host();
        await settle();
        const view = session.view();
        expect(view).toMatchObject({ role: "host", step: 1, phase: "idle" });
        expect(view.ownCode).toMatch(/^NSR1-/);
    });

    it("keeps the same code when going Back from step 2", async () => {
        const fake = fakeLinks();
        session = createNetplaySession({ links: fake.links, build: OURS });
        session.host();
        await settle();
        const code = session.view().ownCode;
        session.next();
        expect(session.view().step).toBe(2);
        session.back();
        expect(session.view()).toMatchObject({ step: 1, ownCode: code });
    });

    it("can connect only once a valid reply code is pasted", async () => {
        const fake = fakeLinks();
        session = createNetplaySession({ links: fake.links, build: OURS });
        session.host();
        await settle();
        session.next();
        expect(session.view().pasteReady).toBe(false);
        await session.paste("hello can u paste the thing");
        expect(session.view()).toMatchObject({ pasteReady: false, pasteProblem: "not-neser" });
        await session.paste(await encodeCode("host", OURS, "x"));
        expect(session.view()).toMatchObject({ pasteReady: false, pasteProblem: "wrong-kind" });
        await session.paste(await encodeCode("reply", OURS, "x"));
        expect(session.view()).toMatchObject({ pasteReady: true, pasteProblem: null });
    });

    it("stops at a reply from another NESER build, naming it", async () => {
        const fake = fakeLinks();
        session = createNetplaySession({ links: fake.links, build: OURS });
        session.host();
        await settle();
        session.next();
        await session.paste(await encodeCode("reply", THEIRS, "x"));
        expect(session.view()).toMatchObject({ phase: "failed", failure: "other-build", theirs: THEIRS, pasteReady: false });
    });

    it("hands the pasted reply to the connection and shows Connecting", async () => {
        const fake = fakeLinks();
        session = createNetplaySession({ links: fake.links, build: OURS });
        session.host();
        await settle();
        session.next();
        await session.paste(await encodeCode("reply", OURS, "answer-sdp"));
        session.connect();
        expect(session.view().phase).toBe("connecting");
    });

    it("connects with a Great response time when round trips are short", async () => {
        const { link } = await hostToConnect(20);
        await vi.advanceTimersByTimeAsync(2000);
        expect(session.view()).toMatchObject({ step: 3, phase: "connected", rtt: 20 });
        expect(link.sentOfType("ready")).toEqual([{ t: "ready", rtt: 20 }]);
    });

    it("connects with a Playable response time between 50 and 120 ms", async () => {
        await hostToConnect(80);
        await vi.advanceTimersByTimeAsync(2000);
        expect(session.view()).toMatchObject({ phase: "connected", rtt: 80 });
    });

    it("refuses a connection that is too slow when connecting, and tells the other side", async () => {
        const { link } = await hostToConnect(200);
        await vi.advanceTimersByTimeAsync(3000);
        expect(session.view()).toMatchObject({ phase: "failed", failure: "too-slow", rtt: 200 });
        expect(link.sentOfType("refuse")).toEqual([{ t: "refuse", rtt: 200 }]);
        expect(link.closed).toBe(true);
    });

    it("lets an open link close itself, so the refusal is delivered before the connection ends", async () => {
        const { fake, link } = await hostToConnect(200);
        await vi.advanceTimersByTimeAsync(3000);
        expect(link.closed).toBe(true);
        expect(fake.hostCancel).not.toHaveBeenCalled();
    });

    it("gives up after 15 seconds when the other player cannot be reached", async () => {
        const fake = fakeLinks();
        session = createNetplaySession({ links: fake.links, build: OURS });
        session.host();
        await settle();
        session.next();
        await session.paste(await encodeCode("reply", OURS, "answer-sdp"));
        session.connect();
        await vi.advanceTimersByTimeAsync(14_900);
        expect(session.view().phase).toBe("connecting");
        await vi.advanceTimersByTimeAsync(200);
        expect(session.view()).toMatchObject({ phase: "failed", failure: "unreachable" });
        expect(fake.hostCancel).toHaveBeenCalled();
    });

    it("starts again from step 1 with a fresh code", async () => {
        const fake = fakeLinks();
        session = createNetplaySession({ links: fake.links, build: OURS });
        session.host();
        await settle();
        const first = session.view().ownCode;
        session.next();
        await session.paste(await encodeCode("reply", OURS, "answer-sdp"));
        session.connect();
        await vi.advanceTimersByTimeAsync(15_100);
        session.startAgain();
        await settle();
        expect(session.view()).toMatchObject({ role: "host", step: 1, phase: "idle", failure: null });
        expect(session.view().ownCode).not.toBe(first);
        expect(fake.hostCancel).toHaveBeenCalled();
    });
});

describe("joining", () => {
    it("moves on only from a valid host code", async () => {
        const fake = fakeLinks();
        session = createNetplaySession({ links: fake.links, build: OURS });
        session.join();
        expect(session.view()).toMatchObject({ role: "join", step: 1, pasteReady: false });
        await session.paste(await encodeCode("reply", OURS, "x"));
        expect(session.view()).toMatchObject({ pasteReady: false, pasteProblem: "wrong-kind" });
        await session.paste(await encodeCode("host", OURS, "offer-sdp"));
        expect(session.view()).toMatchObject({ pasteReady: true, pasteProblem: null });
    });

    it("finds out about another NESER build at step 1, before sending anything back", async () => {
        const fake = fakeLinks();
        const startJoin = vi.spyOn(fake.links, "startJoin");
        session = createNetplaySession({ links: fake.links, build: OURS });
        session.join();
        await session.paste(await encodeCode("host", THEIRS, "offer-sdp"));
        expect(session.view()).toMatchObject({ step: 1, phase: "failed", failure: "other-build", theirs: THEIRS });
        expect(startJoin).not.toHaveBeenCalled();
    });

    it("shows its reply code at step 2 and waits for the host without a time limit", async () => {
        await joinToWaiting();
        expect(session.view()).toMatchObject({ step: 2, phase: "waiting" });
        expect(session.view().ownCode).toMatch(/^NSR1-/);
        await vi.advanceTimersByTimeAsync(10 * 60_000);
        expect(session.view()).toMatchObject({ step: 2, phase: "waiting", failure: null });
    });

    it("is connected once the host says the connection is good, with the host's response time", async () => {
        const fake = await joinToWaiting();
        const link = new FakeLink(20);
        fake.joined.resolve(link);
        await settle();
        link.deliver({ t: "ping", id: 1, at: 5 });
        expect(link.sentOfType("pong")).toEqual([{ t: "pong", id: 1, at: 5 }]);
        link.deliver({ t: "ready", rtt: 31 });
        expect(session.view()).toMatchObject({ step: 3, phase: "connected", rtt: 31 });
    });

    it("is refused when the host found the connection too slow", async () => {
        const fake = await joinToWaiting();
        const link = new FakeLink(200);
        fake.joined.resolve(link);
        await settle();
        link.deliver({ t: "refuse", rtt: 210 });
        expect(session.view()).toMatchObject({ phase: "failed", failure: "too-slow", rtt: 210 });
        expect(link.closed).toBe(true);
    });
});

describe("once connected", () => {
    async function connected(rtt: number) {
        const result = await hostToConnect(rtt);
        await vi.advanceTimersByTimeAsync(2000);
        expect(session.view().phase).toBe("connected");
        return result;
    }

    it("keeps measuring, and stays connected when it becomes too slow, then recovers", async () => {
        const { link } = await connected(20);
        link.rtt = 190;
        await vi.advanceTimersByTimeAsync(8000);
        expect(session.view()).toMatchObject({ phase: "connected", rtt: 190 });
        link.rtt = 25;
        await vi.advanceTimersByTimeAsync(8000);
        expect(session.view()).toMatchObject({ phase: "connected", rtt: 25 });
    });

    it("reports the other player leaving when the link closes", async () => {
        const { link } = await connected(20);
        link.drop();
        expect(session.view()).toMatchObject({ step: 3, phase: "failed", failure: "peer-left" });
    });

    it("reports the other player leaving when they say goodbye", async () => {
        const { link } = await connected(20);
        link.deliver({ t: "bye" });
        expect(session.view()).toMatchObject({ phase: "failed", failure: "peer-left" });
        expect(link.closed).toBe(true);
    });

    it("says goodbye on Disconnect and goes back to the choice", async () => {
        const { link } = await connected(20);
        session.disconnect();
        expect(link.sentOfType("bye")).toHaveLength(1);
        expect(link.closed).toBe(true);
        expect(session.view()).toMatchObject({ role: null, phase: "idle" });
    });

    it("keeps the live link for the next piece of work", async () => {
        const { link } = await connected(20);
        expect(session.link()).toBe(link);
    });
});

describe("cancelling", () => {
    it("cancels a host's pending connection and returns to the choice", async () => {
        const fake = fakeLinks();
        session = createNetplaySession({ links: fake.links, build: OURS });
        session.host();
        await settle();
        session.cancel();
        expect(fake.hostCancel).toHaveBeenCalled();
        expect(session.view()).toMatchObject({ role: null });
    });

    it("cancels a joiner who is waiting", async () => {
        const fake = await joinToWaiting();
        session.cancel();
        expect(fake.joinCancel).toHaveBeenCalled();
        const link = new FakeLink(20);
        fake.joined.resolve(link);
        await settle();
        expect(link.closed).toBe(true);
        expect(session.view()).toMatchObject({ role: null, phase: "idle" });
    });

    it("tells every change to its listener", async () => {
        const fake = fakeLinks();
        session = createNetplaySession({ links: fake.links, build: OURS });
        const seen: number[] = [];
        session.subscribe((view) => seen.push(view.step));
        session.host();
        await settle();
        session.next();
        expect(seen).toContain(2);
    });
});

describe("a connection that cannot start", () => {
    it("tells a host whose browser cannot make a connection", async () => {
        const fake = fakeLinks();
        fake.links.startHost = () => Promise.reject(new Error("no WebRTC"));
        session = createNetplaySession({ links: fake.links, build: OURS });
        session.host();
        await settle();
        expect(session.view()).toMatchObject({ phase: "failed", failure: "unreachable" });
    });

    it("tells a joiner whose pasted code cannot be answered", async () => {
        const fake = fakeLinks();
        fake.links.startJoin = () => Promise.reject(new Error("bad SDP"));
        session = createNetplaySession({ links: fake.links, build: OURS });
        session.join();
        await session.paste(await encodeCode("host", OURS, "offer-sdp"));
        session.next();
        await settle();
        expect(session.view()).toMatchObject({ phase: "failed", failure: "unreachable" });
    });
});

describe("a link that goes silent", () => {
    it("shows the wait for the unanswered round trip, turning Too slow while still connected", async () => {
        const { link } = await hostToConnect(20);
        await vi.advanceTimersByTimeAsync(2000);
        link.rtt = 1_000_000;
        await vi.advanceTimersByTimeAsync(4000);
        expect(session.view().phase).toBe("connected");
        expect(session.view().rtt).toBeGreaterThan(120);
    });
});
