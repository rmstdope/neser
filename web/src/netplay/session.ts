import { THIS_BUILD, type BuildId } from "./build_id";
import { crc32, MAX_PACKAGE_BYTES, packGame, unpackGame, type GamePackage } from "./game_package";
import type { Signaling, SignalingSocket, ServerMessage } from "./liaison";
import type { HostStart, JoinStart, PeerLink, PeerLinks } from "./peer_link";
import { isTooSlow, median } from "./response_time";
import { ROOM_CODE_LENGTH, showRoomCode, tidyRoomCode } from "./room_code";

/**
 * Connecting two browsers for a network game (nr-ayk5.3), without the page: the host's and the
 * joiner's steps, the response-time check that decides whether the connection is good enough, and
 * what happens when it ends. The Play together window draws `view()`; this module owns every bit
 * of state behind it, and the open link once connected.
 *
 * The two meet through liaison (nr-z4nw, `liaison.ts`): the host opens a room and shows its code;
 * the joiner joins it by that code, and liaison checks the two run the same build. Once someone
 * has joined, the host sends its offer through the room as `signal {offer}` and the joiner its
 * answer as `signal {answer}`; once the direct link is open each side closes its connection to
 * liaison. From the swap on, the other side leaving the room is no longer news: it is what the
 * other side does once its link is open, and the link itself says whether the two connected. A
 * host whose room ran out, or whose joiner left before the swap, gets a fresh room by itself. The
 * joiner keeps one connection to liaison across its tries at a code, so liaison's limit on wrong
 * codes holds; Start again begins with a fresh one. liaison may give the room a TURN relay with
 * `created` and `joined` (nr-1wbx); both connections are offered it, the view says whether one was
 * tried (which can't-connect message to show) and whether the link went through it.
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
 * `stop` is either side's Cancel. `arrived` carries how long loading the game took the joiner. The
 * host confirms it still runs the game it sent and says `start`: the joiner then closes its own
 * game and sets the received one up, which takes about that long, and the host waits half the
 * round trip it measured before the game filled the link plus that time, so both counts begin
 * together. Until `start` the joiner's own game is untouched. Nothing the other side sends
 * is trusted: a message that does not describe itself is ignored, and a transfer that does not
 * add up did not arrive.
 *
 * Playing together (nr-ayk5.5) rides the same link. `start` carries the input delay the host
 * chose from the response time, so both play with the same one. The lockstep packets go as bytes
 * led by transfer id 0, which no game transfer uses; `resume` and `reset` are the play messages,
 * handed to `onPlay` as they are. `end` says the other player ended playing together, where `bye`
 * says they left.
 *
 * Resyncing (nr-ayk5.6) rides it too. `state` announces the host's save state (the frame it is for,
 * its size and CRC) and its bytes follow in pieces led by transfer id `STATE_ID`, unacknowledged:
 * a save state is far smaller than what a channel queues, and the channel is ordered and reliable.
 * The joiner hands it to `onPlay` whole, or as `bytes: null` when it does not add up; `resynced` is
 * the joiner's word that it has loaded it, a play message like the others; and `unsynced` ends
 * playing together on both sides because the games could not be made the same again.
 */

export type Role = "host" | "join";
/**
 * Why the joiner's step failed in a way the code field can fix: no room by that code, a full one,
 * liaison out of reach or busy, or the host leaving before the two connected.
 */
export type JoinProblem = "room-not-found" | "room-full" | "service" | "busy" | "host-left";
/** Why the host has a new room: the old one ran out, or the joiner left before the two connected. */
export type RoomNote = "expired" | "left";
/**
 * Why an attempt ended for good: another build, no direct path, too slow, the other side leaving
 * once connected, liaison out of reach or busy (host), or too many wrong codes (joiner).
 */
export type Failure = "other-build" | "unreachable" | "too-slow" | "peer-left" | "service" | "busy" | "too-many-attempts";
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
    /** Frames between a press and the frame it is played on, the same on both sides. */
    inputDelay: number;
}

/** What the two games say to each other while playing together, besides the lockstep packets. */
/** Each names the frame it is for: the one both games stopped before. */
export type PlayMessage =
    | { t: "resume"; at: number }
    | { t: "reset"; soft: boolean; at: number }
    | { t: "resynced"; at: number };
/** The host's save state for the frame `at`, or null when what came did not add up. */
export interface ResyncState {
    t: "state";
    at: number;
    bytes: Uint8Array | null;
}
/**
 * The frames a second this side's computer can keep up while playing together, or null at full
 * speed (nr-ayk5.7): both games run at the slower of the two sides'. For no particular frame.
 */
export type SpeedMessage = { t: "speed"; fps: number | null };
/** A lockstep packet (bytes), a play message, a speed message, or the host's state for a resync. */
export type PlayData = Uint8Array | PlayMessage | SpeedMessage | ResyncState;
/**
 * How playing together ended from the other side: they ended it, they left (closed the page, lost
 * the link), or the games could not be resynced.
 */
export type Leaving = "ended" | "left" | "unsynced";

export interface NetplayView {
    /** Who this browser is; null on the first screen, where the player chooses. */
    role: Role | null;
    step: 1 | 2;
    /** The host's room code, as shown (`K7Q-M4X`); null until liaison has opened the room. */
    room: string | null;
    /** Why the host's room is a new one, until the attempt ends. */
    roomNote: RoomNote | null;
    /** What went wrong with the joiner's last try at a code; null before one, and while trying. */
    joinProblem: JoinProblem | null;
    /**
     * Host: `opening` (asking liaison for a room), `waiting` (the code shown), `connecting` (someone
     * joined). Joiner: `idle` (typing a code), `connecting` (after Join), `waiting` (linked,
     * waiting for the host's check). Both: `connected`, `failed`.
     */
    phase: "idle" | "opening" | "waiting" | "connecting" | "connected" | "failed";
    failure: Failure | null;
    /** The other player's build, when it is not ours. */
    theirs: BuildId | null;
    /** The response time shown, in whole milliseconds. */
    rtt: number | null;
    /** The game on its way, while it is. */
    sending: Sending | null;
    notice: SendingNotice | null;
    /** How the other player went, once they have (failure "peer-left"). */
    leaving: Leaving | null;
    /**
     * This attempt's connection was given the room's relay (nr-1wbx): which message says the two
     * couldn't connect. A failure before any connection was made, or with no relay, says none was available.
     */
    relayTried: boolean;
    /** The two connected through the relay; kept by a failure, so a too-slow refusal can say so. */
    relayed: boolean;
}

export interface NetplaySession {
    view(): NetplayView;
    subscribe(listener: (view: NetplayView) => void): void;
    host(): void;
    join(): void;
    /** The joiner's Join: tries `code`, as typed, once it has all six characters. */
    joinRoom(code: string): void;
    disconnect(): void;
    /** Ends whatever is under way and goes back to the choice: Cancel, or the window closing before connecting. */
    cancel(): void;
    startAgain(): void;
    /** The open link to the other browser, once connected. */
    link(): PeerLink | null;
    /** The host sends `game`, titled `title`, and both start once it has arrived. */
    startGame(game: GamePackage, title: string): void;
    /** Either side's Cancel while the game is on its way; the connection stays. Says whether it cancelled. */
    cancelSending(): boolean;
    /** Ends playing together: the other side is told it was ended, not left. */
    end(): void;
    /** Sends a lockstep packet, or a play message, to the other side. */
    sendPacket(bytes: Uint8Array): void;
    sendPlay(message: PlayMessage | SpeedMessage): void;
    /** Sends the host's save state for the frame `at`, for a resync. */
    sendState(at: number, bytes: Uint8Array): void;
    /** Ends playing together on both sides: the games could not be resynced. */
    endUnsynced(): void;
    /** Hears the other side's lockstep packets, play messages and resync states. */
    onPlay(listener: (data: PlayData) => void): void;
}

/** The transfer id that leads a lockstep packet; game transfers count from 1. */
const PLAY_PACKET_ID = 0;
/** The transfer id that leads a piece of a resync state: one no game transfer reaches. */
const STATE_ID = 0xffff_ffff;
const FRAME_MS = 1000 / 60;
const MIN_INPUT_DELAY = 2;
const MAX_INPUT_DELAY = 6;

/**
 * The input delay for a response time: enough frames for a press to cross the link one way, plus
 * one to spare, within two to six.
 */
export function inputDelayFor(rttMs: number): number {
    const frames = Math.ceil(rttMs / 2 / FRAME_MS) + 1;
    return Math.min(MAX_INPUT_DELAY, Math.max(MIN_INPUT_DELAY, frames));
}

/**
 * How long the two have, from the joiner being in the room, for the connection and its check:
 * "about 10 seconds at most" before both are told their networks don't allow it (nr-ri8n). It
 * leaves room for both browsers' address gathering (3 s each at most) when outpost is out of reach.
 */
export const CONNECT_LIMIT_MS = 10_000;
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
/** The longest the host waits for the joiner to set the game up, whatever the joiner says. */
export const MAX_SETUP_MS = 2000;

type Message =
    | { t: "ping"; id: number; at: number }
    | { t: "pong"; id: number; at: number }
    | { t: "ready"; rtt: number }
    | { t: "refuse"; rtt: number }
    | { t: "bye" }
    | { t: "end" }
    | { t: "game"; id: number; name: string; total: number; crc: number }
    | { t: "got"; id: number; bytes: number }
    | { t: "arrived"; id: number; ms: number }
    | { t: "broken"; id: number }
    | { t: "stop"; id: number }
    | { t: "start"; id: number; delay: number }
    | { t: "state"; at: number; total: number; crc: number }
    | { t: "unsynced" }
    | PlayMessage
    | SpeedMessage;

/** The host's side of a game on its way. */
type Outgoing = {
    id: number;
    game: GamePackage;
    packed: Uint8Array;
    sent: number;
    acked: number;
    /** Whether `start` has been sent. */
    started: boolean;
    /** The response time before sending: the game queued on the link slows every round trip during it. */
    rtt: number;
};
/** The joiner's side; `game` is set once it has arrived and passed its check. */
type Incoming = { id: number; total: number; crc: number; buffer: Uint8Array; received: number; game: GamePackage | null };

const IDLE: NetplayView = {
    role: null,
    step: 1,
    room: null,
    roomNote: null,
    joinProblem: null,
    phase: "idle",
    failure: null,
    theirs: null,
    rtt: null,
    sending: null,
    notice: null,
    leaving: null,
    relayTried: false,
    relayed: false,
};

const isCount = (value: unknown): value is number => typeof value === "number" && Number.isInteger(value) && value >= 0;

/** A speed as this build sends it: whole frames a second, at least one. */
const isRate = (value: unknown): value is number => typeof value === "number" && Number.isInteger(value) && value >= 1;

/** Whether a message from the other side is a play or speed message as this build sends them. */
function isPlayMessage(message: { t: string } & Record<string, unknown>): message is PlayMessage | SpeedMessage {
    if (message.t === "speed") return message.fps === null || isRate(message.fps);
    if (!isCount(message.at)) return false;
    if (message.t === "resume" || message.t === "resynced") return true;
    return message.t === "reset" && typeof message.soft === "boolean";
}

export function createNetplaySession({
    signaling,
    links,
    build = THIS_BUILD,
    checkGame = async () => 0,
    confirmStart = () => true,
    onStart = () => {},
}: {
    signaling: Signaling;
    links: PeerLinks;
    build?: BuildId;
    /**
     * The joiner's test that what arrived is a game this browser can run, without disturbing its
     * own: how long loading it took, in ms, or null when it cannot run it.
     */
    checkGame?: (game: GamePackage) => Promise<number | null>;
    /** Whether the host still runs the game it sent, asked before telling the joiner to start. */
    confirmStart?: (game: GamePackage) => boolean;
    /** Both sides, together: the joiner sets the game up and both count down. */
    onStart?: (start: NetplayStart) => void;
}): NetplaySession {
    let state: NetplayView = { ...IDLE };
    const listeners: ((view: NetplayView) => void)[] = [];
    /** Bumped whenever an attempt ends, so the late results of an abandoned one are dropped. */
    let attempt = 0;
    /** The connection to liaison of the attempt under way, until the direct link is open. */
    let socket: SignalingSocket | null = null;
    /** The offer and answer have been swapped: the other side leaving the room is no longer news. */
    let swapped = false;
    /** The TURN relay liaison gave the room of the attempt under way, or none. */
    let relay: RTCIceServer[] = [];
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
    let playListener: (data: PlayData) => void = () => {};
    /** A resync state on its way in. */
    let incomingState: { at: number; total: number; crc: number; buffer: Uint8Array; received: number } | null = null;
    const timers = new Set<ReturnType<typeof setTimeout>>();
    const intervals = new Set<ReturnType<typeof setInterval>>();

    const now = () => performance.now();

    function set(changes: Partial<NetplayView>) {
        state = { ...state, ...changes };
        for (const listener of listeners) listener(state);
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
        incomingState = null;
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
        closeSocket();
        swapped = false;
        relay = [];
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
        if (data.length < 4) return;
        const id = new DataView(data.buffer, data.byteOffset, data.byteLength).getUint32(0, true);
        if (id === PLAY_PACKET_ID) return playListener(data.slice(4));
        if (id === STATE_ID) return receiveStatePiece(data.subarray(4));
        if (!into || into.game) return;
        // Chunks of an attempt already given up (cancelled, or replaced) are still on their way.
        if (id !== into.id) return;
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
            (ms) => arrivedChecked(link, into, mine, ms === null ? null : game, ms ?? 0),
            () => arrivedChecked(link, into, mine, null, 0)
        );
    }

    function stateAnnounced(message: Extract<Message, { t: "state" }>) {
        const { at, total, crc } = message;
        if (!isCount(at)) return;
        incomingState = null;
        if (!isCount(total) || total === 0 || total > MAX_PACKAGE_BYTES || !isCount(crc)) {
            return playListener({ t: "state", at, bytes: null });
        }
        incomingState = { at, total, crc, buffer: new Uint8Array(total), received: 0 };
    }

    function receiveStatePiece(piece: Uint8Array) {
        const into = incomingState;
        if (!into) return;
        if (into.received + piece.length > into.total) {
            incomingState = null;
            return playListener({ t: "state", at: into.at, bytes: null });
        }
        into.buffer.set(piece, into.received);
        into.received += piece.length;
        if (into.received < into.total) return;
        incomingState = null;
        playListener({ t: "state", at: into.at, bytes: crc32(into.buffer) === into.crc ? into.buffer : null });
    }

    function arrivedChecked(link: PeerLink, into: Incoming, mine: number, game: GamePackage | null, ms: number) {
        if (mine !== attempt || incoming !== into) return;
        if (!game) return refuseIncoming(link, into.id);
        into.game = game;
        send(link, { t: "arrived", id: into.id, ms: Math.max(0, Math.round(ms)) });
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
            else if (message.t === "start" && incoming.game && isCount(message.delay) && message.delay >= 1) {
                const game = incoming.game;
                // Told before the send's view clears, so what it starts is in place when the window hears.
                onStart({ role: "join", game, inputDelay: Math.min(message.delay, MAX_INPUT_DELAY) });
                transferEnded(null);
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
        else if (message.t === "arrived" && !outgoing.started && outgoing.acked === outgoing.packed.length) {
            // A host that stopped its game, or loaded another, has nothing to start from.
            if (!confirmStart(outgoing.game)) {
                send(link, { t: "broken", id });
                return transferEnded("not-arrived");
            }
            outgoing.started = true;
            clearTimeout(stallTimer);
            const inputDelay = inputDelayFor(outgoing.rtt);
            send(link, { t: "start", id, delay: inputDelay });
            const setup = isCount(message.ms) ? Math.min(message.ms, MAX_SETUP_MS) : 0;
            const game = outgoing.game;
            const mine = attempt;
            // The joiner hears `start` about half a round trip from now and then sets the game up.
            timers.add(
                setTimeout(() => {
                    if (mine !== attempt || outgoing?.id !== id) return;
                    onStart({ role: "host", game, inputDelay });
                    transferEnded(null);
                }, Math.round(outgoing.rtt / 2) + setup)
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
        set({ phase: "failed", failure, sending: null, notice: null, ...extra });
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
        set({ step: 2, phase: "connected", rtt });
        measureLive(link);
    }

    /** Listens on a fresh link: pongs feed `onSample`, and the other player's leaving ends it. */
    function adopt(link: PeerLink, mine: number, onSample: (rtt: number) => void, onMessage: (m: Message) => void) {
        live = link;
        set({ relayed: link.relayed });
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
            else if (message.t === "bye") peerLeft("left");
            else if (message.t === "end") peerLeft("ended");
            else if (message.t === "unsynced") peerLeft("unsynced");
            else if (message.t === "state") {
                if (state.phase === "connected") stateAnnounced(message);
            } else if (["resume", "reset", "resynced", "speed"].includes(message.t)) {
                if (state.phase === "connected" && isPlayMessage(message)) playListener(message);
            }
            else if (["game", "got", "arrived", "broken", "stop", "start"].includes(message.t)) {
                if (state.phase === "connected") onTransfer(link, message);
            } else onMessage(message);
        });
        link.onClosed(() => {
            if (mine === attempt) peerLeft("left");
        });
    }

    function peerLeft(leaving: Leaving) {
        if (state.phase === "connected") fail("peer-left", { leaving });
        else if (state.phase === "connecting" || state.phase === "waiting") fail("unreachable");
    }

    function liveSample(rtt: number) {
        if (state.phase !== "connected") return;
        samples = [...samples.slice(1 - LIVE_SAMPLES), rtt];
        showLive();
    }

    function closeSocket() {
        socket?.close();
        socket = null;
    }

    /** Opens a connection to liaison for this attempt; `hear` and `lost` are told only while it is current. */
    function openSocket(mine: number, hear: (m: ServerMessage) => void, lost: () => void, then: (s: SignalingSocket) => void, failed: () => void) {
        signaling.open().then(
            (opened) => {
                if (mine !== attempt) return opened.close();
                socket = opened;
                opened.onMessage((m) => {
                    if (mine === attempt && socket === opened) hear(m);
                });
                opened.onClosed(() => {
                    if (mine !== attempt || socket !== opened) return;
                    socket = null;
                    lost();
                });
                then(opened);
            },
            () => {
                if (mine === attempt) failed();
            }
        );
    }

    /** No direct link within the limit, from the moment the joiner is in the room. */
    function startConnectLimit(mine: number) {
        timers.add(setTimeout(() => mine === attempt && fail("unreachable"), CONNECT_LIMIT_MS));
    }

    const sdpIn = (data: unknown, key: "offer" | "answer"): string | null => {
        if (typeof data !== "object" || data === null) return null;
        const sdp = (data as Record<string, unknown>)[key];
        return typeof sdp === "string" ? sdp : null;
    };

    function beginHost(note: RoomNote | null = null) {
        const mine = attempt;
        set({ ...IDLE, role: "host", phase: "opening", roomNote: note });
        openSocket(
            mine,
            (m) => hostHears(m, mine),
            // Once the two have swapped, liaison is no longer needed; before, its loss ends the attempt.
            () => swapped || fail("service"),
            (s) => s.send({ type: "create", build }),
            () => fail("service")
        );
    }

    /** A fresh room, the old one gone: it ran out, or its joiner left before the two connected. */
    function renewRoom(note: RoomNote) {
        teardown();
        beginHost(note);
    }

    function hostHears(m: ServerMessage, mine: number) {
        if (m.type === "created" && state.phase === "opening") {
            relay = m.iceServers ?? [];
            set({ phase: "waiting", room: showRoomCode(tidyRoomCode(m.room)) });
        } else if (m.type === "peer-joined" && state.phase === "waiting") {
            set({ phase: "connecting" });
            startConnectLimit(mine);
            set({ relayTried: relay.length > 0 });
            links.startHost(relay).then(
                (start) => {
                    if (mine !== attempt) return start.cancel();
                    hostStart = start;
                    socket?.send({ type: "signal", data: { offer: start.sdp } });
                },
                // No working WebRTC: the nearest agreed message is that it could not connect.
                () => mine === attempt && fail("unreachable")
            );
        } else if (m.type === "signal" && state.phase === "connecting" && hostStart && !swapped) {
            const answer = sdpIn(m.data, "answer");
            if (answer === null) return;
            swapped = true;
            hostStart.accept(answer).then(
                (link) => {
                    if (mine !== attempt) return link.close();
                    closeSocket();
                    hostCheck(link, mine);
                },
                () => mine === attempt && fail("unreachable")
            );
        } else if (m.type === "peer-left" && !swapped) {
            renewRoom("left");
        } else if (m.type === "error" && !swapped) {
            if (m.reason === "room-expired") renewRoom("expired");
            else if (m.reason === "server-busy") fail("busy");
            else fail("unreachable");
        }
    }

    function beginJoin() {
        set({ ...IDLE, role: "join" });
    }

    /** A try at a code that the field can put right: back to typing, saying why. */
    function joinTryFailed(problem: JoinProblem) {
        stopClocks();
        set({ phase: "idle", joinProblem: problem });
    }

    function joinerHears(m: ServerMessage, mine: number) {
        if (state.phase !== "connecting") return;
        if (m.type === "joined") {
            relay = m.iceServers ?? [];
            startConnectLimit(mine);
        } else if (m.type === "signal" && !joinStart && !swapped) {
            const offer = sdpIn(m.data, "offer");
            if (offer === null) return;
            swapped = true;
            set({ relayTried: relay.length > 0 });
            links.startJoin(offer, relay).then(
                (start) => {
                    if (mine !== attempt) return start.cancel();
                    joinStart = start;
                    socket?.send({ type: "signal", data: { answer: start.sdp } });
                    start.link.then(
                        (link) => {
                            if (mine !== attempt) return link.close();
                            closeSocket();
                            set({ phase: "waiting" });
                            joinerListen(link, mine);
                        },
                        () => mine === attempt && fail("unreachable")
                    );
                },
                // An offer the browser refuses, or no working WebRTC.
                () => mine === attempt && fail("unreachable")
            );
        } else if (m.type === "peer-left" && !swapped) {
            teardown();
            joinTryFailed("host-left");
        } else if (m.type === "error" && !swapped) {
            if (m.reason === "room-not-found") joinTryFailed("room-not-found");
            else if (m.reason === "room-full") joinTryFailed("room-full");
            else if (m.reason === "server-busy") joinTryFailed("busy");
            else if (m.reason === "other-build" && m.theirs) fail("other-build", { theirs: m.theirs });
            else if (m.reason === "too-many-attempts") fail("too-many-attempts");
            else if (m.reason === "room-expired") {
                teardown();
                joinTryFailed("host-left");
            } else fail("unreachable");
        }
    }

    function joinRoom(code: string) {
        const chars = tidyRoomCode(code);
        if (state.role !== "join" || state.phase !== "idle" || chars.length !== ROOM_CODE_LENGTH) return;
        const mine = attempt;
        set({ phase: "connecting", joinProblem: null, relayTried: false });
        const ask = (s: SignalingSocket) => s.send({ type: "join", room: showRoomCode(chars), build });
        if (socket) return ask(socket);
        openSocket(
            mine,
            (m) => joinerHears(m, mine),
            () => {
                if (swapped || state.phase !== "connecting") return;
                teardown();
                joinTryFailed("service");
            },
            ask,
            () => joinTryFailed("service")
        );
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

    return {
        view: () => state,
        subscribe: (next) => {
            listeners.push(next);
        },
        host: () => {
            teardown();
            beginHost();
        },
        join: () => {
            teardown();
            beginJoin();
        },
        joinRoom,
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
            outgoing = { id, game, packed, sent: 0, acked: 0, started: false, rtt: state.rtt ?? 0 };
            send(live, { t: "game", id, name: title, total: packed.length, crc: crc32(packed) });
            set({ sending: { direction: "send", name: title, done: 0, total: packed.length }, notice: null });
            watchStall(live, id);
            pump(live);
        },
        end: () => {
            if (live) send(live, { t: "end" });
            teardown();
            set({ ...IDLE });
        },
        sendPacket: (bytes) => {
            if (state.phase !== "connected" || !live) return;
            const frame = new Uint8Array(4 + bytes.length);
            new DataView(frame.buffer).setUint32(0, PLAY_PACKET_ID, true);
            frame.set(bytes, 4);
            live.send(frame);
        },
        sendPlay: (message) => {
            if (state.phase === "connected" && live) send(live, message);
        },
        sendState: (at, bytes) => {
            if (state.phase !== "connected" || !live) return;
            send(live, { t: "state", at, total: bytes.length, crc: crc32(bytes) });
            for (let sent = 0; sent < bytes.length; sent += CHUNK_BYTES) {
                const piece = bytes.subarray(sent, sent + CHUNK_BYTES);
                const frame = new Uint8Array(4 + piece.length);
                new DataView(frame.buffer).setUint32(0, STATE_ID, true);
                frame.set(piece, 4);
                live.send(frame);
            }
        },
        endUnsynced: () => {
            if (state.phase !== "connected") return;
            if (live) send(live, { t: "unsynced" });
            fail("peer-left", { leaving: "unsynced" });
        },
        onPlay: (listener) => {
            playListener = listener;
        },
        cancelSending: () => {
            const id = outgoing?.id ?? incoming?.id;
            if (id === undefined || !live) return false;
            // Once all of it has arrived the start is under way on both sides; a stop then would
            // race it and could leave one side starting alone.
            if (outgoing && outgoing.acked === outgoing.packed.length) return false;
            if (incoming && incoming.received === incoming.total) return false;
            send(live, { t: "stop", id });
            transferEnded(null);
            return true;
        },
    };
}
