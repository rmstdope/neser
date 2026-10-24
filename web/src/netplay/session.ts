import { THIS_BUILD, type BuildId } from "./build_id";
import { encodeCode, readCode, type CodeKind } from "./code";
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
 */

export type Role = "host" | "join";
export type PasteProblem = "not-neser" | "wrong-kind";
export type Failure = "other-build" | "unreachable" | "too-slow" | "peer-left";

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
}

/** How long the host waits, from Connect, for the connection and its check. */
export const CONNECT_LIMIT_MS = 15_000;
const CHECK_SAMPLES = 5;
const CHECK_INTERVAL_MS = 200;
const LIVE_INTERVAL_MS = 1000;
const LIVE_SAMPLES = 5;

type Message =
    | { t: "ping"; id: number; at: number }
    | { t: "pong"; id: number; at: number }
    | { t: "ready"; rtt: number }
    | { t: "refuse"; rtt: number }
    | { t: "bye" };

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
};

export function createNetplaySession({ links, build = THIS_BUILD }: { links: PeerLinks; build?: BuildId }): NetplaySession {
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
        set({ phase: "failed", failure, pasteReady: false, ...extra });
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
        link.onMessage((text) => {
            if (mine !== attempt) return;
            let message: Message;
            try {
                message = JSON.parse(text) as Message;
            } catch {
                return;
            }
            if (message.t === "ping") send(link, { t: "pong", id: message.id, at: message.at });
            else if (message.t === "pong") {
                unanswered.delete(message.id);
                onSample(Math.round(now() - message.at));
            }
            else if (message.t === "bye") peerLeft();
            else onMessage(message);
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
    };
}
