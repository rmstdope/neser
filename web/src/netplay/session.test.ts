import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ClientMessage, ServerMessage, Signaling, SignalingSocket } from "./liaison";
import type { HostStart, JoinStart, PeerLink, PeerLinks } from "./peer_link";
import { CONNECT_LIMIT_MS, createNetplaySession, type NetplaySession } from "./session";

const OURS = { version: "1.3.0", commit: "4f2a9c1" };
const THEIRS = { version: "1.3.0", commit: "b81e07d" };

/** The other browser at the end of a fake link: it answers pings after `rtt` ms, and logs the rest. */
class FakeLink implements PeerLink {
    relayed = false;
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
    /** The relay each start was given, in order. */
    const hostRelays: RTCIceServer[][] = [];
    const joinRelays: RTCIceServer[][] = [];
    const links: PeerLinks = {
        startHost: async (relay): Promise<HostStart> => {
            hostRelays.push(relay);
            return { sdp: `offer-sdp-${++offers}`, accept: vi.fn(() => accepted.promise), cancel: hostCancel };
        },
        startJoin: async (hostSdp: string, relay): Promise<JoinStart> => {
            joinRelays.push(relay);
            return { sdp: `answer-to-${hostSdp}`, link: joined.promise, cancel: joinCancel };
        },
    };
    return { links, accepted, joined, hostCancel, joinCancel, hostRelays, joinRelays };
}

/** This browser's connection to liaison, which the test speaks for. */
class FakeSocket implements SignalingSocket {
    sent: ClientMessage[] = [];
    closed = false;
    private messageListener: (m: ServerMessage) => void = () => {};
    private closedListener: () => void = () => {};
    send(message: ClientMessage) {
        this.sent.push(message);
    }
    onMessage(listener: (m: ServerMessage) => void) {
        this.messageListener = listener;
    }
    onClosed(listener: () => void) {
        this.closedListener = listener;
    }
    close() {
        this.closed = true;
    }
    says(message: ServerMessage) {
        this.messageListener(message);
    }
    /** Liaison, or the network, ended the connection. */
    ends() {
        this.closedListener();
    }
    sentOfType<T extends ClientMessage["type"]>(type: T) {
        return this.sent.filter((m): m is Extract<ClientMessage, { type: T }> => m.type === type);
    }
}

/** Liaison: every connection the session opens, or a refusal while `reachable` is false. */
function fakeLiaison() {
    const sockets: FakeSocket[] = [];
    const liaison = {
        reachable: true,
        sockets,
        last: () => sockets[sockets.length - 1],
        signaling: {
            open: async (): Promise<SignalingSocket> => {
                if (!liaison.reachable) throw new Error("unreachable");
                const socket = new FakeSocket();
                sockets.push(socket);
                return socket;
            },
        } as Signaling,
    };
    return liaison;
}

/** Lets promises and zero-delay work finish without moving the clock. */
async function settle() {
    for (let i = 0; i < 20; i++) await new Promise((r) => setImmediate(r));
}

let session: NetplaySession;
let liaison: ReturnType<typeof fakeLiaison>;
let fake: ReturnType<typeof fakeLinks>;

beforeEach(() => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date", "performance"] });
    liaison = fakeLiaison();
    fake = fakeLinks();
    session = createNetplaySession({ signaling: liaison.signaling, links: fake.links, build: OURS });
});
afterEach(() => {
    session?.cancel();
    vi.useRealTimers();
});

/** The host's room is open, showing K7Q-M4X. */
async function hostWaiting() {
    session.host();
    await settle();
    liaison.last().says({ type: "created", room: "K7Q-M4X" });
    return liaison.last();
}

/** Someone typed the host's code and the two swap offer and answer; the link is not open yet. */
async function hostSwapped() {
    const socket = await hostWaiting();
    socket.says({ type: "peer-joined" });
    await settle();
    socket.says({ type: "signal", data: { answer: "answer-sdp" } });
    await settle();
    return socket;
}

async function hostToConnect(rtt: number) {
    const socket = await hostSwapped();
    const link = new FakeLink(rtt);
    fake.accepted.resolve(link);
    await settle();
    return { socket, link };
}

/** The joiner typed `code` and pressed Join; liaison has the request. */
async function joinTried(code = "k7q-m4x") {
    session.join();
    session.joinRoom(code);
    await settle();
    return liaison.last();
}

/** The joiner is in the room, has the host's offer, and has sent its answer back. */
async function joinToWaiting() {
    const socket = await joinTried();
    socket.says({ type: "joined" });
    socket.says({ type: "signal", data: { offer: "offer-sdp" } });
    await settle();
    return socket;
}

describe("hosting", () => {
    it("asks liaison for a room for this build, and shows its code", async () => {
        session.host();
        expect(session.view()).toMatchObject({ role: "host", step: 1, phase: "opening", room: null });
        await settle();
        expect(liaison.last().sent).toEqual([{ type: "create", build: OURS }]);
        liaison.last().says({ type: "created", room: "K7Q-M4X" });
        expect(session.view()).toMatchObject({ phase: "waiting", room: "K7Q-M4X", roomNote: null });
    });

    it("waits for the other player without a time limit of its own", async () => {
        await hostWaiting();
        await vi.advanceTimersByTimeAsync(14 * 60_000);
        expect(session.view()).toMatchObject({ phase: "waiting", room: "K7Q-M4X" });
    });

    it("sends its offer through the room once someone has typed the code, and shows Connecting", async () => {
        const socket = await hostWaiting();
        socket.says({ type: "peer-joined" });
        expect(session.view().phase).toBe("connecting");
        await settle();
        expect(socket.sentOfType("signal")).toEqual([{ type: "signal", data: { offer: "offer-sdp-1" } }]);
    });

    it("hands the joiner's answer to the connection, and leaves liaison once the link is open", async () => {
        const { socket } = await hostToConnect(20);
        expect(socket.closed).toBe(true);
        await vi.advanceTimersByTimeAsync(2000);
        expect(session.view()).toMatchObject({ step: 2, phase: "connected", rtt: 20 });
    });

    it("connects with a Great response time when round trips are short", async () => {
        const { link } = await hostToConnect(20);
        await vi.advanceTimersByTimeAsync(2000);
        expect(session.view()).toMatchObject({ step: 2, phase: "connected", rtt: 20 });
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
        const { link } = await hostToConnect(200);
        await vi.advanceTimersByTimeAsync(3000);
        expect(link.closed).toBe(true);
        expect(fake.hostCancel).not.toHaveBeenCalled();
    });

    // About 10 seconds at most, on both sides (nr-ri8n): the limit is pinned here, not read back.
    it("gives up 10 seconds after someone typed the code when the other player cannot be reached", async () => {
        const socket = await hostWaiting();
        socket.says({ type: "peer-joined" });
        await settle();
        socket.says({ type: "signal", data: { answer: "answer-sdp" } });
        await vi.advanceTimersByTimeAsync(9_900);
        expect(session.view().phase).toBe("connecting");
        await vi.advanceTimersByTimeAsync(200);
        expect(session.view()).toMatchObject({ phase: "failed", failure: "unreachable" });
        expect(fake.hostCancel).toHaveBeenCalled();
        expect(socket.closed).toBe(true);
    });

    it("gets a new code by itself when the old one runs out, saying so", async () => {
        const old = await hostWaiting();
        old.says({ type: "error", reason: "room-expired" });
        expect(old.closed).toBe(true);
        expect(session.view()).toMatchObject({ phase: "opening", room: null, roomNote: "expired" });
        await settle();
        expect(liaison.sockets).toHaveLength(2);
        liaison.last().says({ type: "created", room: "T9W-2HB" });
        expect(session.view()).toMatchObject({ phase: "waiting", room: "T9W-2HB", roomNote: "expired" });
    });

    it("gets a new code by itself when the joiner leaves before the two swapped, saying so", async () => {
        const old = await hostWaiting();
        old.says({ type: "peer-joined" });
        await settle();
        old.says({ type: "peer-left" });
        expect(fake.hostCancel).toHaveBeenCalled();
        await settle();
        liaison.last().says({ type: "created", room: "T9W-2HB" });
        expect(session.view()).toMatchObject({ phase: "waiting", room: "T9W-2HB", roomNote: "left", failure: null });
    });

    it("does not take the joiner leaving the room once the two have swapped as leaving the game", async () => {
        const socket = await hostSwapped();
        socket.says({ type: "peer-left" });
        socket.ends();
        expect(session.view()).toMatchObject({ phase: "connecting", failure: null });
        expect(liaison.sockets).toHaveLength(1);
    });

    it("cannot start when liaison cannot be reached", async () => {
        liaison.reachable = false;
        session.host();
        await settle();
        expect(session.view()).toMatchObject({ role: "host", phase: "failed", failure: "service" });
    });

    it("cannot go on when its connection to liaison drops while it waits", async () => {
        const socket = await hostWaiting();
        socket.ends();
        expect(session.view()).toMatchObject({ phase: "failed", failure: "service" });
    });

    it("says liaison is busy when it has too many rooms open", async () => {
        session.host();
        await settle();
        liaison.last().says({ type: "error", reason: "server-busy" });
        expect(session.view()).toMatchObject({ phase: "failed", failure: "busy" });
        expect(liaison.last().closed).toBe(true);
    });

    it("tries again with a fresh connection to liaison", async () => {
        liaison.reachable = false;
        session.host();
        await settle();
        liaison.reachable = true;
        session.startAgain();
        await settle();
        liaison.last().says({ type: "created", room: "K7Q-M4X" });
        expect(session.view()).toMatchObject({ role: "host", phase: "waiting", failure: null, room: "K7Q-M4X" });
    });

    it("starts again with a fresh room after failing to reach the other player", async () => {
        await hostSwapped();
        await vi.advanceTimersByTimeAsync(CONNECT_LIMIT_MS + 100);
        session.startAgain();
        await settle();
        expect(liaison.sockets).toHaveLength(2);
        liaison.last().says({ type: "created", room: "T9W-2HB" });
        expect(session.view()).toMatchObject({ role: "host", step: 1, phase: "waiting", failure: null, roomNote: null, room: "T9W-2HB" });
    });
});

describe("joining", () => {
    it("starts with nothing tried", () => {
        session.join();
        expect(session.view()).toMatchObject({ role: "join", step: 1, phase: "idle", joinProblem: null });
    });

    it("tries nothing until the code has all six characters", async () => {
        session.join();
        session.joinRoom("K7Q-M4");
        await settle();
        expect(liaison.sockets).toHaveLength(0);
        expect(session.view().phase).toBe("idle");
    });

    it("asks liaison for the room by its code, tidied, with this build", async () => {
        const socket = await joinTried(" k7q m4x ");
        expect(session.view().phase).toBe("connecting");
        expect(socket.sent).toEqual([{ type: "join", room: "K7Q-M4X", build: OURS }]);
    });

    it("answers the host's offer through the room", async () => {
        const socket = await joinToWaiting();
        expect(socket.sentOfType("signal")).toEqual([{ type: "signal", data: { answer: "answer-to-offer-sdp" } }]);
    });

    it("leaves liaison once the link is open, and waits for the host's check", async () => {
        const socket = await joinToWaiting();
        fake.joined.resolve(new FakeLink(20));
        await settle();
        expect(socket.closed).toBe(true);
        expect(session.view()).toMatchObject({ step: 1, phase: "waiting" });
    });

    it("is connected once the host says the connection is good, with the host's response time", async () => {
        await joinToWaiting();
        const link = new FakeLink(20);
        fake.joined.resolve(link);
        await settle();
        link.deliver({ t: "ping", id: 1, at: 5 });
        expect(link.sentOfType("pong")).toEqual([{ t: "pong", id: 1, at: 5 }]);
        link.deliver({ t: "ready", rtt: 31 });
        expect(session.view()).toMatchObject({ step: 2, phase: "connected", rtt: 31 });
    });

    it("is refused when the host found the connection too slow", async () => {
        await joinToWaiting();
        const link = new FakeLink(200);
        fake.joined.resolve(link);
        await settle();
        link.deliver({ t: "refuse", rtt: 210 });
        expect(session.view()).toMatchObject({ phase: "failed", failure: "too-slow", rtt: 210 });
        expect(link.closed).toBe(true);
    });

    it("gives up 10 seconds after getting into the room when the host cannot be reached", async () => {
        await joinToWaiting();
        await vi.advanceTimersByTimeAsync(9_900);
        expect(session.view().phase).not.toBe("failed");
        await vi.advanceTimersByTimeAsync(200);
        expect(session.view()).toMatchObject({ phase: "failed", failure: "unreachable" });
        expect(fake.joinCancel).toHaveBeenCalled();
    });

    it("does not take the host leaving the room once the two have swapped as leaving", async () => {
        const socket = await joinToWaiting();
        socket.says({ type: "peer-left" });
        socket.ends();
        expect(session.view()).toMatchObject({ phase: "connecting", failure: null, joinProblem: null });
    });

    for (const [reason, problem] of [
        ["room-not-found", "room-not-found"],
        ["room-full", "room-full"],
        ["server-busy", "busy"],
    ] as const) {
        it(`goes back to the code, saying ${problem}, when liaison answers ${reason}, and tries again on the same connection`, async () => {
            const socket = await joinTried();
            socket.says({ type: "error", reason });
            expect(session.view()).toMatchObject({ phase: "idle", joinProblem: problem, failure: null });
            session.joinRoom("K7QM4Y");
            expect(session.view()).toMatchObject({ phase: "connecting", joinProblem: null });
            await settle();
            expect(liaison.sockets).toHaveLength(1);
            expect(socket.sentOfType("join").map((m) => m.room)).toEqual(["K7Q-M4X", "K7Q-M4Y"]);
        });
    }

    it("stops at a host running another NESER build, naming it", async () => {
        const socket = await joinTried();
        socket.says({ type: "error", reason: "other-build", theirs: THEIRS });
        expect(session.view()).toMatchObject({ phase: "failed", failure: "other-build", theirs: THEIRS });
        expect(socket.closed).toBe(true);
    });

    it("stops after too many wrong codes, and Start again begins with a fresh connection", async () => {
        const socket = await joinTried();
        socket.says({ type: "error", reason: "too-many-attempts" });
        expect(session.view()).toMatchObject({ phase: "failed", failure: "too-many-attempts" });
        session.startAgain();
        expect(session.view()).toMatchObject({ role: "join", phase: "idle", failure: null });
        session.joinRoom("K7QM4X");
        await settle();
        expect(liaison.sockets).toHaveLength(2);
    });

    it("says the host closed the game when the host leaves before the two swapped", async () => {
        const socket = await joinTried();
        socket.says({ type: "joined" });
        socket.says({ type: "peer-left" });
        expect(session.view()).toMatchObject({ phase: "idle", joinProblem: "host-left", failure: null });
        expect(socket.closed).toBe(true);
        session.joinRoom("K7QM4X");
        await settle();
        expect(liaison.sockets).toHaveLength(2);
    });

    it("says liaison cannot be reached, keeping the code to try again", async () => {
        liaison.reachable = false;
        await joinTried();
        expect(session.view()).toMatchObject({ role: "join", phase: "idle", joinProblem: "service", failure: null });
        liaison.reachable = true;
        session.joinRoom("K7QM4X");
        await settle();
        expect(session.view().phase).toBe("connecting");
    });

    it("says liaison cannot be reached when its connection drops during a try", async () => {
        const socket = await joinTried();
        socket.ends();
        expect(session.view()).toMatchObject({ phase: "idle", joinProblem: "service" });
    });

    it("opens a fresh connection for the next try when liaison dropped it while the joiner was typing", async () => {
        const socket = await joinTried();
        socket.says({ type: "error", reason: "room-not-found" });
        socket.ends();
        expect(session.view()).toMatchObject({ phase: "idle", joinProblem: "room-not-found" });
        session.joinRoom("K7QM4Y");
        await settle();
        expect(liaison.sockets).toHaveLength(2);
    });
});

const RELAY: RTCIceServer = { urls: ["turn:outpost.rmstdope.eu:3478?transport=udp"], username: "u", credential: "c" };

describe("the relay (nr-1wbx)", () => {
    it("gives the host's connection the relay liaison sent with the room", async () => {
        session.host();
        await settle();
        const socket = liaison.last();
        socket.says({ type: "created", room: "K7Q-M4X", iceServers: [RELAY] });
        expect(session.view().relayOffered).toBe(true);
        socket.says({ type: "peer-joined" });
        await settle();
        expect(fake.hostRelays).toEqual([[RELAY]]);
    });

    it("gives the joiner's connection the relay liaison sent on joining", async () => {
        const socket = await joinTried();
        socket.says({ type: "joined", iceServers: [RELAY] });
        expect(session.view().relayOffered).toBe(true);
        socket.says({ type: "signal", data: { offer: "offer-sdp" } });
        await settle();
        expect(fake.joinRelays).toEqual([[RELAY]]);
    });

    it("connects directly only when liaison sent no relay", async () => {
        await hostSwapped();
        await joinToWaiting();
        expect(fake.hostRelays).toEqual([[]]);
        expect(fake.joinRelays).toEqual([[]]);
        expect(session.view().relayOffered).toBe(false);
    });

    it("uses the relay of the room it has now, when a new room replaced the old", async () => {
        session.host();
        await settle();
        liaison.last().says({ type: "created", room: "K7Q-M4X", iceServers: [RELAY] });
        liaison.last().says({ type: "error", reason: "room-expired" });
        await settle();
        const socket = liaison.last();
        socket.says({ type: "created", room: "ABC-DEF" });
        expect(session.view().relayOffered).toBe(false);
        socket.says({ type: "peer-joined" });
        await settle();
        expect(fake.hostRelays).toEqual([[]]);
    });

    it("says the connection is relayed when its link went through the relay, on both sides", async () => {
        await hostSwapped();
        const link = new FakeLink(20);
        link.relayed = true;
        fake.accepted.resolve(link);
        await settle();
        await vi.advanceTimersByTimeAsync(2000);
        expect(session.view()).toMatchObject({ phase: "connected", relayed: true });

        session.cancel();
        fake = fakeLinks();
        session = createNetplaySession({ signaling: liaison.signaling, links: fake.links, build: OURS });
        await joinToWaiting();
        const joinLink = new FakeLink(20);
        joinLink.relayed = true;
        fake.joined.resolve(joinLink);
        await settle();
        joinLink.deliver({ t: "ready", rtt: 90 });
        expect(session.view()).toMatchObject({ phase: "connected", relayed: true });
    });

    it("says a direct connection is not relayed", async () => {
        await hostToConnect(20);
        await vi.advanceTimersByTimeAsync(2000);
        expect(session.view()).toMatchObject({ phase: "connected", relayed: false });
    });

    it("keeps saying a refused connection was relayed, for its advice", async () => {
        await joinToWaiting();
        const link = new FakeLink(200);
        link.relayed = true;
        fake.joined.resolve(link);
        await settle();
        link.deliver({ t: "refuse", rtt: 210 });
        expect(session.view()).toMatchObject({ phase: "failed", failure: "too-slow", relayed: true });
    });

    it("says whether a relay was offered when the two could not connect at all", async () => {
        const socket = await joinTried();
        socket.says({ type: "joined", iceServers: [RELAY] });
        await vi.advanceTimersByTimeAsync(10_100);
        expect(session.view()).toMatchObject({ phase: "failed", failure: "unreachable", relayOffered: true });
        session.startAgain();
        expect(session.view()).toMatchObject({ relayOffered: false, relayed: false });
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
        expect(session.view()).toMatchObject({ step: 2, phase: "failed", failure: "peer-left" });
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
    it("closes a host's room and pending connection and returns to the choice", async () => {
        const socket = await hostWaiting();
        socket.says({ type: "peer-joined" });
        await settle();
        session.cancel();
        expect(fake.hostCancel).toHaveBeenCalled();
        expect(socket.closed).toBe(true);
        expect(session.view()).toMatchObject({ role: null, phase: "idle" });
    });

    it("cancels a joiner who is waiting", async () => {
        const socket = await joinToWaiting();
        session.cancel();
        expect(fake.joinCancel).toHaveBeenCalled();
        expect(socket.closed).toBe(true);
        const link = new FakeLink(20);
        fake.joined.resolve(link);
        await settle();
        expect(link.closed).toBe(true);
        expect(session.view()).toMatchObject({ role: null, phase: "idle" });
    });

    it("closes a connection to liaison that opens only after Cancel", async () => {
        session.host();
        session.cancel();
        await settle();
        expect(liaison.last().closed).toBe(true);
        expect(session.view()).toMatchObject({ role: null, phase: "idle" });
    });
});

describe("a connection that cannot start", () => {
    it("tells a host whose browser cannot make a connection", async () => {
        fake.links.startHost = () => Promise.reject(new Error("no WebRTC"));
        const socket = await hostWaiting();
        socket.says({ type: "peer-joined" });
        await settle();
        expect(session.view()).toMatchObject({ phase: "failed", failure: "unreachable" });
    });

    it("tells a joiner whose browser cannot answer the host's offer", async () => {
        fake.links.startJoin = () => Promise.reject(new Error("bad SDP"));
        await joinToWaiting();
        expect(session.view()).toMatchObject({ phase: "failed", failure: "unreachable" });
    });

    it("ignores signals that carry no offer or answer", async () => {
        const socket = await joinTried();
        socket.says({ type: "joined" });
        socket.says({ type: "signal", data: { answer: "x" } });
        socket.says({ type: "signal", data: "nonsense" });
        await settle();
        expect(socket.sentOfType("signal")).toEqual([]);
        expect(session.view().phase).toBe("connecting");
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

describe("subscribers", () => {
    it("tells every subscriber of each change", async () => {
        const first: string[] = [];
        const second: string[] = [];
        session.subscribe((v) => first.push(v.phase));
        session.subscribe((v) => second.push(v.phase));
        session.join();
        expect(first.length).toBeGreaterThan(0);
        expect(second).toEqual(first);
    });
});
