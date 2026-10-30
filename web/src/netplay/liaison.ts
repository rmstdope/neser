import type { BuildId } from "./build_id";

/**
 * The meeting service for network play (nr-z4nw): liaison, github.com/rmstdope/liaison. Two
 * browsers meet in a room named by a short code and swap, through it, the offer and answer that
 * open their direct connection; liaison never sees the game. This is the only file that touches
 * `WebSocket`; the session drives it through `Signaling`.
 */

export const LIAISON_URL = "wss://liaison.rmstdope.eu/ws";
/** How long reaching liaison may take before network play says it can't start. */
export const OPEN_LIMIT_MS = 10_000;

export type ClientMessage =
    | { type: "create"; build: BuildId }
    | { type: "join"; room: string; build: BuildId }
    | { type: "signal"; data: unknown };

export type ErrorReason =
    | "bad-message"
    | "already-in-room"
    | "not-in-room"
    | "no-peer"
    | "room-not-found"
    | "room-full"
    | "other-build"
    | "server-busy"
    | "too-many-attempts"
    | "too-many-signals"
    | "room-expired";

/**
 * What liaison says. The TURN relay it may offer (`iceServers`) is left out: this build connects
 * the two browsers directly only.
 */
export type ServerMessage =
    | { type: "created"; room: string }
    | { type: "joined" }
    | { type: "peer-joined" }
    | { type: "peer-left" }
    | { type: "signal"; data: unknown }
    | { type: "error"; reason: string; theirs?: BuildId };

/** An open connection to liaison. `onClosed` is told once if liaison or the network ends it. */
export interface SignalingSocket {
    send(message: ClientMessage): void;
    onMessage(listener: (message: ServerMessage) => void): void;
    onClosed(listener: () => void): void;
    /** Closes it from this side; `onClosed` is not told. */
    close(): void;
}

export interface Signaling {
    /** Connects to liaison; fails when it cannot be reached within `OPEN_LIMIT_MS`. */
    open(): Promise<SignalingSocket>;
}

const isObject = (value: unknown): value is Record<string, unknown> => typeof value === "object" && value !== null && !Array.isArray(value);

function isBuild(value: unknown): value is BuildId {
    return isObject(value) && typeof value.version === "string" && typeof value.commit === "string";
}

/** A message from liaison, read as untrusted: anything that is not exactly one is null. */
export function parseServerMessage(text: string): ServerMessage | null {
    let message: unknown;
    try {
        message = JSON.parse(text);
    } catch {
        return null;
    }
    if (!isObject(message)) return null;
    switch (message.type) {
        case "created":
            return typeof message.room === "string" ? { type: "created", room: message.room } : null;
        case "joined":
        case "peer-joined":
        case "peer-left":
            return { type: message.type };
        case "signal":
            return "data" in message ? { type: "signal", data: message.data } : null;
        case "error": {
            if (typeof message.reason !== "string") return null;
            if (message.theirs === undefined) return { type: "error", reason: message.reason };
            return isBuild(message.theirs)
                ? { type: "error", reason: message.reason, theirs: { version: message.theirs.version, commit: message.theirs.commit } }
                : null;
        }
        default:
            return null;
    }
}

function wrap(ws: WebSocket): SignalingSocket {
    let closedHere = false;
    let closedListener: () => void = () => {};
    ws.addEventListener("close", () => {
        if (!closedHere) closedListener();
    });
    return {
        send: (message) => ws.send(JSON.stringify(message)),
        onMessage: (listener) =>
            ws.addEventListener("message", (event) => {
                if (typeof event.data !== "string") return;
                const message = parseServerMessage(event.data);
                if (message) listener(message);
            }),
        onClosed: (listener) => {
            closedListener = listener;
        },
        close: () => {
            closedHere = true;
            ws.close();
        },
    };
}

export function liaisonSignaling(url = LIAISON_URL, makeSocket: (url: string) => WebSocket = (u) => new WebSocket(u)): Signaling {
    return {
        open: () =>
            new Promise((resolve, reject) => {
                let ws: WebSocket;
                try {
                    ws = makeSocket(url);
                } catch (error) {
                    return reject(error);
                }
                const timer = setTimeout(() => {
                    ws.close();
                    reject(new Error("liaison did not answer"));
                }, OPEN_LIMIT_MS);
                ws.addEventListener(
                    "open",
                    () => {
                        clearTimeout(timer);
                        resolve(wrap(ws));
                    },
                    { once: true }
                );
                ws.addEventListener(
                    "close",
                    () => {
                        clearTimeout(timer);
                        reject(new Error("liaison could not be reached"));
                    },
                    { once: true }
                );
            }),
    };
}
