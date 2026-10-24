import { THIS_BUILD, type BuildId } from "./build_id";
import { encodeCode, readCode, type CodeKind } from "./code";
import { crc32, MAX_PACKAGE_BYTES, packGame, unpackGame, type GamePackage } from "./game_package";
import type { HostStart, JoinStart, PeerLink, PeerLinks } from "./peer_link";
import { isTooSlow, median } from "./response_time";

/**
 * Connecting two browsers for a network game (nr-ayk5.3), without the page: the host's and the
 * joiner's steps, the codes they swap, the response-time check that decides whether the
 * connection is good enough, and what happens when it ends. The Play together window draws
 * `view()`; this module owns every bit of state behind it, and the open link once connected.
 *
 * The two browsers talk in JSON text over the link: each answers `ping` with `pong`; the host
 * measures `CHECK_SAMPLES` round trips on connecting and sends `ready` or `refuse` with their
 * median, so both sides agree; once connected each side measures once a second; `bye` says the
 * other player has left.
 *
 * Once connected the host can send its game (nr-ayk5.4): `game` announces it (its title, size and
 * CRC), the bytes follow as binary chunks each led by the transfer's id (u32, little-endian), and
 * the joiner acknowledges every chunk with `got`, which both moves the host's progress and keeps
 * at most `WINDOW_BYTES` in flight. The joiner checks what came and answers `arrived` or `broken`;
 * `stop` is either side's Cancel; `start` tells the joiner to begin. Nothing the other side sends
 * is trusted: a message that does not describe itself is ignored, and a transfer that does not
 * add up did not arrive.
 */

export type Role = "host" | "join";
export type PasteProblem = "not-neser" | "wrong-kind";
export type Failure = "other-build" | "unreachable" | "too-slow" | "peer-left";
/** The line on the connected step about the last attempt to send the game. */
export type SendingNotice = "host-stopped" | "joiner-stopped" | "not-arrived";

export interface Sending {
    direction: "send" | "receive";
    /** The game's title, as the window names it. */
    name: string;
    /** Bytes the joiner has received; the host's count follows the joiner's acknowledgements. */
    done: number;
    total: number;
}

/** The game has arrived and been checked: both sides begin from it. */
export interface NetplayStart {
    role: Role;
    game: GamePackage;
}

export interface NetplayView {
    /** Who this browser is; null on the first screen, where the player chooses. */
    role: Role | null;
    step: 1 | 2 | 3;
    /** The code this browser shows (host step 1, joiner step 2); null while it is being made. */
    ownCode: string | null;
    /** What is wrong with the pasted text, if anything; null when it is fine or empty. */
    pasteProblem: PasteProblem | null;
    /** Whether the pasted text is a code this step can use. */
    pasteReady: boolean;
    phase: "idle" | "connecting" | "waiting" | "connected" | "failed";
    failure: Failure | null;
    /** The other player's build, when it is not ours. */
    theirs: BuildId | null;
    /** The response time shown, in whole milliseconds. */
    rtt: number | null;
    /** The game on its way, while it is. */
    sending: Sending | null;
    notice: SendingNotice | null;
}

export interface NetplaySession {
    view(): NetplayView;
    subscribe(listener: (view: NetplayView) => void): void;
    host(): void;
    join(): void;
    next(): void;
    back(): void;
    paste(text: string): Promise<void>;
    connect(): void;
    disconnect(): void;
    /** Ends whatever is under way and goes back to the choice: Cancel, or the window closing before connecting. */
    cancel(): void;
    startAgain(): void;
    /** The open link to the other browser, once connected. */
    link(): PeerLink | null;
    /** The host sends `game`, titled `title`, and both start once it has arrived. */
    startGame(game: GamePackage, title: string): void;
    /** Either side's Cancel while the game is on its way; the connection stays. */
    cancelSending(): void;
}

/** How long the host waits, from Connect, for the connection and its check. */
export const CONNECT_LIMIT_MS = 15_000;
const CHECK_SAMPLES = 5;
const CHECK_INTERVAL_MS = 200;
const LIVE_INTERVAL_MS = 1000;
const LIVE_SAMPLES = 5;
/** The largest piece of the game sent at once, well under what any browser's data channel takes. */
export const CHUNK_BYTES = 16 * 1024;
/** How much of the game may be on its way, unacknowledged, at once. */
export const WINDOW_BYTES = 256 * 1024;
/** A send in which nothing moves for this long has broken off. */
export const STALL_MS = 10_000;
const MAX_TITLE_LENGTH = 255;

type Message =
    | { t: "ping"; id: number; at: number }
    | { t: "pong"; id: number; at: number }
    | { t: "ready"; rtt: number }
    | { t: "refuse"; rtt: number }
    | { t: "bye" }
    | { t: "game"; id: number; name: string; total: number; crc: number }
    | { t: "got"; id: number; bytes: number }
    | { t: "arrived"; id: number }
    | { t: "broken"; id: number }
    | { t: "stop"; id: number }
    | { t: "start"; id: number };

/** The host's side of a game on its way. */
type Outgoing = { id: number; game: GamePackage; packed: Uint8Array; sent: number; acked: number; arrived: boolean };
/** The joiner's side; `game` is set once it has arrived and passed its check. */
type Incoming = { id: number; total: number; crc: number; buffer: Uint8Array; received: number; game: GamePackage | null };

const IDLE: NetplayView = {
    role: null,
    step: 1,
    ownCode: null,
    pasteProblem: null,
    pasteReady: false,
    phase: "idle",
    failure: null,
    theirs: null,
    rtt: null,
    sending: null,
    notice: null,
};

const isCount = (value: unknown): value is number => typeof value === "number" && Number.isInteger(value) && value >= 0;

export function createNetplaySession({
    links,
    build = THIS_BUILD,
    checkGame = async () => true,
    onStart = () => {},
}: {
    links: PeerLinks;
    build?: BuildId;
    /** The joiner's test that what arrived is a game this browser can run, without disturbing its own. */
    checkGame?: (game: GamePackage) => Promise<boolean>;
    onStart?: (start: NetplayStart) => void;
}): NetplaySession {
    let state: NetplayView = { ...IDLE };
    let listener: (view: NetplayView) => void = () => {};
    /** Bumped whenever an attempt ends, so the late results of an abandoned one are dropped. */
    let attempt = 0;
    let pastedSdp: string | null = null;
    let pasteCheck = 0;
    let hostStart: HostStart | null = null;
    let joinStart: JoinStart | null = null;
    let live: PeerLink | null = null;
    let samples: number[] = [];
    /** When each ping still waiting for its pong was sent, by id: a silent link shows as slow. */
    let unanswered = new Map<number, number>();
    let pingId = 0;
    let transferId = 0;
    let outgoing: Outgoing | null = null;
    let incoming: Incoming | null = null;
    let stallTimer: ReturnType<typeof setTimeout> | undefined;
    const timers = new Set<ReturnType<typeof setTimeout>>();
    const intervals = new Set<ReturnType<typeof setInterval>>();

    const now = () => performance.now();

    function set(changes: Partial<NetplayView>) {
        state = { ...state, ...changes };
        listener(state);
    }

    function stopClocks() {
        for (const timer of timers) clearTimeout(timer);
        for (const interval of intervals) clearInterval(interval);
        timers.clear();
        intervals.clear();
    }

    /** Ends the attempt: the clocks stop, and any link or pending connection is closed. */
    function teardown() {
        attempt++;
        stopClocks();
        endTransfer();
        // An open link closes its own connection once its last message is out; cancelling the
        // connection under it would drop that message (a refusal, a goodbye).
        if (live) live.close();
        else {
            hostStart?.cancel();
            joinStart?.cancel();
        }
        hostStart = null;
        joinStart = null;
        live = null;
        pastedSdp = null;
        samples = [];
        unanswered = new Map();
    }

    function endTransfer() {
        clearTimeout(stallTimer);
        outgoing = null;
        incoming = null;
    }

    /** The current attempt at sending ends, saying `notice` (or nothing) on the connected step. */
    function transferEnded(notice: SendingNotice | null) {
        endTransfer();
        set({ sending: null, notice });
    }

    /** Restarts the clock after which a send in which nothing moves has broken off. */
    function watchStall(link: PeerLink, id: number) {
        clearTimeout(stallTimer);
        stallTimer = setTimeout(() => {
            if (outgoing?.id !== id && incoming?.id !== id) return;
            send(link, { t: "broken", id });
            transferEnded("not-arrived");
        }, STALL_MS);
    }

    /** Sends as much of the game as the window allows. */
    function pump(link: PeerLink) {
        const out = outgoing;
        if (!out) return;
        while (out.sent < out.packed.length && out.sent - out.acked < WINDOW_BYTES) {
            const piece = out.packed.subarray(out.sent, out.sent + CHUNK_BYTES);
            const frame = new Uint8Array(4 + piece.length);
            new DataView(frame.buffer).setUint32(0, out.id, true);
            frame.set(piece, 4);
            link.send(frame);
            out.sent += piece.length;
        }
    }

    function receiveBytes(link: PeerLink, data: Uint8Array) {
        const into = incoming;
        if (!into || into.game || data.length < 4) return;
        // Chunks of an attempt already given up (cancelled, or replaced) are still on their way.
        if (new DataView(data.buffer, data.byteOffset, data.byteLength).getUint32(0, true) !== into.id) return;
        const piece = data.subarray(4);
        if (into.received + piece.length > into.total) return refuseIncoming(link, into.id);
        into.buffer.set(piece, into.received);
        into.received += piece.length;
        send(link, { t: "got", id: into.id, bytes: into.received });
        set({ sending: { ...state.sending!, done: into.received } });
        if (into.received < into.total) return watchStall(link, into.id);
        clearTimeout(stallTimer);
        const game = crc32(into.buffer) === into.crc ? unpackGame(into.buffer) : null;
        if (!game) return refuseIncoming(link, into.id);
        const mine = attempt;
        checkGame(game).then(
            (ok) => arrivedChecked(link, into, mine, ok ? game : null),
            () => arrivedChecked(link, into, mine, null)
        );
    }

    function arrivedChecked(link: PeerLink, into: Incoming, mine: number, game: GamePackage | null) {
        if (mine !== attempt || incoming !== into) return;
        if (!game) return refuseIncoming(link, into.id);
        into.game = game;
        send(link, { t: "arrived", id: into.id });
    }

    function refuseIncoming(link: PeerLink, id: number) {
        send(link, { t: "broken", id });
        transferEnded("not-arrived");
    }

    function onTransfer(link: PeerLink, message: Message) {
        if (!("id" in message) || !isCount(message.id)) return;
        const id = message.id;
        if (state.role === "join") {
            if (message.t === "game") return announced(link, message);
            if (incoming?.id !== id) return;
            if (message.t === "stop") transferEnded("host-stopped");
            else if (message.t === "broken") transferEnded("not-arrived");
            else if (message.t === "start" && incoming.game) {
                const game = incoming.game;
                transferEnded(null);
                onStart({ role: "join", game });
            }
            return;
        }
        if (outgoing?.id !== id) return;
        if (message.t === "got" && isCount(message.bytes)) {
            outgoing.acked = Math.min(Math.max(outgoing.acked, message.bytes), outgoing.sent);
            set({ sending: { ...state.sending!, done: outgoing.acked } });
            watchStall(link, id);
            pump(link);
        } else if (message.t === "stop") transferEnded("joiner-stopped");
        else if (message.t === "broken") transferEnded("not-arrived");
        else if (message.t === "arrived" && !outgoing.arrived && outgoing.acked === outgoing.packed.length) {
            outgoing.arrived = true;
            clearTimeout(stallTimer);
            send(link, { t: "start", id });
            const game = outgoing.game;
            const mine = attempt;
            // The joiner hears `start` about half a round trip from now; the host waits as long.
            timers.add(
                setTimeout(() => {
                    if (mine !== attempt || outgoing?.id !== id) return;
                    transferEnded(null);
                    onStart({ role: "host", game });
                }, Math.round((state.rtt ?? 0) / 2))
            );
        }
    }

    function announced(link: PeerLink, message: Extract<Message, { t: "game" }>) {
        const { id, total, crc, name } = message;
        const fits = isCount(total) && total > 0 && total <= MAX_PACKAGE_BYTES;
        if (!fits || !isCount(crc) || typeof name !== "string" || name.length > MAX_TITLE_LENGTH) {
            return refuseIncoming(link, id);
        }
        endTransfer();
        incoming = { id, total, crc, buffer: new Uint8Array(total), received: 0, game: null };
        set({ sending: { direction: "receive", name, done: 0, total }, notice: null });
        watchStall(link, id);
    }

    function send(link: PeerLink, message: Message) {
        link.send(JSON.stringify(message));
    }

    function ping(link: PeerLink) {
        const at = now();
        unanswered.set(++pingId, at);
        send(link, { t: "ping", id: pingId, at });
    }

    function fail(failure: Failure, extra: Partial<NetplayView> = {}) {
        teardown();
        set({ phase: "failed", failure, pasteReady: false, sending: null, notice: null, ...extra });
    }

    function measureLive(link: PeerLink) {
        samples = [];
        unanswered = new Map();
        intervals.add(
            setInterval(() => {
                showLive();
                ping(link);
            }, LIVE_INTERVAL_MS)
        );
    }

    /**
     * The response time shown once connected: the median of the last few round trips, or how long
     * the oldest unanswered ping has waited when that is longer, so a link that has gone silent
     * turns Too slow rather than keep showing its last good time.
     */
    function showLive() {
        if (state.phase !== "connected") return;
        const measured = samples.length > 0 ? median(samples) : (state.rtt ?? 0);
        const oldest = Math.min(...unanswered.values());
        const waited = Number.isFinite(oldest) ? now() - oldest : 0;
        set({ rtt: Math.round(Math.max(measured, waited)) });
    }

    function connected(link: PeerLink, rtt: number) {
        stopClocks();
        set({ step: 3, phase: "connected", rtt });
        measureLive(link);
    }

    /** Listens on a fresh link: pongs feed `onSample`, and the other player's leaving ends it. */
    function adopt(link: PeerLink, mine: number, onSample: (rtt: number) => void, onMessage: (m: Message) => void) {
        live = link;
        link.onMessage((data) => {
            if (mine !== attempt) return;
            if (typeof data !== "string") {
                if (state.phase === "connected") receiveBytes(link, data);
                return;
            }
            let message: Message;
            try {
                message = JSON.parse(data) as Message;
            } catch {
                return;
            }
            if (typeof message !== "object" || message === null) return;
            if (message.t === "ping") send(link, { t: "pong", id: message.id, at: message.at });
            else if (message.t === "pong") {
                unanswered.delete(message.id);
                onSample(Math.round(now() - message.at));
            }
            else if (message.t === "bye") peerLeft();
            else if (["game", "got", "arrived", "broken", "stop", "start"].includes(message.t)) {
                if (state.phase === "connected") onTransfer(link, message);
            } else onMessage(message);
        });
        link.onClosed(() => {
            if (mine === attempt) peerLeft();
        });
    }

    function peerLeft() {
        if (state.phase === "connected") fail("peer-left");
        else if (state.phase === "connecting" || state.phase === "waiting") fail("unreachable");
    }

    function liveSample(rtt: number) {
        if (state.phase !== "connected") return;
        samples = [...samples.slice(1 - LIVE_SAMPLES), rtt];
        showLive();
    }

    function beginHost() {
        const mine = attempt;
        set({ ...IDLE, role: "host" });
        (async () => {
            const start = await links.startHost();
            if (mine !== attempt) return start.cancel();
            hostStart = start;
            const code = await encodeCode("host", build, start.sdp);
            if (mine === attempt) set({ ownCode: code });
        })().catch(() => {
            // No working WebRTC, or no deflate: the nearest agreed message is that it could not connect.
            if (mine === attempt) fail("unreachable");
        });
    }

    function beginJoin() {
        set({ ...IDLE, role: "join" });
    }

    function hostCheck(link: PeerLink, mine: number) {
        const check: number[] = [];
        const sampler = setInterval(() => ping(link), CHECK_INTERVAL_MS);
        intervals.add(sampler);
        ping(link);
        adopt(
            link,
            mine,
            (rtt) => {
                if (state.phase === "connected") return liveSample(rtt);
                if (state.phase !== "connecting") return;
                check.push(rtt);
                if (check.length < CHECK_SAMPLES) return;
                const verdict = Math.round(median(check));
                if (isTooSlow(verdict)) {
                    send(link, { t: "refuse", rtt: verdict });
                    fail("too-slow", { rtt: verdict });
                } else {
                    send(link, { t: "ready", rtt: verdict });
                    connected(link, verdict);
                }
            },
            () => {}
        );
    }

    function joinerListen(link: PeerLink, mine: number) {
        adopt(link, mine, liveSample, (message) => {
            if (message.t === "ready" && state.phase === "waiting") connected(link, message.rtt);
            else if (message.t === "refuse") fail("too-slow", { rtt: message.rtt });
        });
    }

    async function paste(text: string) {
        const expected: CodeKind = state.role === "host" ? "reply" : "host";
        const mine = ++pasteCheck;
        if (text.trim() === "") {
            pastedSdp = null;
            set({ pasteProblem: null, pasteReady: false });
            return;
        }
        const check = await readCode(text, expected, build);
        if (mine !== pasteCheck) return;
        if (check.ok) {
            pastedSdp = check.sdp;
            set({ pasteProblem: null, pasteReady: true });
        } else if (check.problem === "other-build") {
            fail("other-build", { theirs: check.theirs, pasteProblem: null });
        } else {
            pastedSdp = null;
            set({ pasteProblem: check.problem, pasteReady: false });
        }
    }

    return {
        view: () => state,
        subscribe: (next) => {
            listener = next;
        },
        host: () => {
            teardown();
            beginHost();
        },
        join: () => {
            teardown();
            beginJoin();
        },
        next: () => {
            if (state.role === "host" && state.step === 1) {
                set({ step: 2, pasteProblem: null, pasteReady: false });
                return;
            }
            if (state.role !== "join" || state.step !== 1 || !pastedSdp) return;
            const mine = attempt;
            const hostSdp = pastedSdp;
            set({ step: 2, phase: "waiting", ownCode: null, pasteReady: false });
            (async () => {
                const start = await links.startJoin(hostSdp);
                if (mine !== attempt) return start.cancel();
                joinStart = start;
                start.link.then(
                    (link) => {
                        if (mine !== attempt) return link.close();
                        joinerListen(link, mine);
                    },
                    () => {
                        if (mine === attempt) fail("unreachable");
                    }
                );
                const code = await encodeCode("reply", build, start.sdp);
                if (mine === attempt) set({ ownCode: code });
            })().catch(() => {
                // A code whose connection details the browser refuses, or no working WebRTC.
                if (mine === attempt) fail("unreachable");
            });
        },
        back: () => {
            if (state.role === "host" && state.step === 2 && state.phase === "idle") {
                pastedSdp = null;
                set({ step: 1, pasteProblem: null, pasteReady: false });
            }
        },
        paste,
        connect: () => {
            if (state.role !== "host" || !hostStart || !pastedSdp || state.phase !== "idle") return;
            const mine = attempt;
            set({ phase: "connecting", pasteReady: false });
            timers.add(setTimeout(() => mine === attempt && fail("unreachable"), CONNECT_LIMIT_MS));
            hostStart.accept(pastedSdp).then(
                (link) => {
                    if (mine !== attempt) return link.close();
                    hostCheck(link, mine);
                },
                () => {
                    if (mine === attempt) fail("unreachable");
                }
            );
        },
        disconnect: () => {
            if (live) send(live, { t: "bye" });
            teardown();
            set({ ...IDLE });
        },
        cancel: () => {
            if (live) send(live, { t: "bye" });
            teardown();
            set({ ...IDLE });
        },
        startAgain: () => {
            const role = state.role;
            teardown();
            if (role === "host") beginHost();
            else beginJoin();
        },
        link: () => (state.phase === "connected" ? live : null),
        startGame: (game, title) => {
            if (state.role !== "host" || state.phase !== "connected" || state.sending || !live) return;
            const packed = packGame(game);
            const id = ++transferId;
            endTransfer();
            outgoing = { id, game, packed, sent: 0, acked: 0, arrived: false };
            send(live, { t: "game", id, name: title, total: packed.length, crc: crc32(packed) });
            set({ sending: { direction: "send", name: title, done: 0, total: packed.length }, notice: null });
            watchStall(live, id);
            pump(live);
        },
        cancelSending: () => {
            const id = outgoing?.id ?? incoming?.id;
            if (id === undefined || !live) return;
            send(live, { t: "stop", id });
            transferEnded(null);
        },
    };
}
