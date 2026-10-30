import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { LIAISON_URL, OPEN_LIMIT_MS, liaisonSignaling, parseServerMessage, type SignalingSocket } from "./liaison";

/** A WebSocket the test opens, closes and talks through by hand. */
class FakeSocket extends EventTarget {
    readyState = 0;
    sent: string[] = [];
    closed = false;
    constructor(readonly url: string) {
        super();
    }
    send(text: string) {
        this.sent.push(text);
    }
    close() {
        this.closed = true;
        this.readyState = 3;
    }
    opens() {
        this.readyState = 1;
        this.dispatchEvent(new Event("open"));
    }
    fails() {
        this.dispatchEvent(new Event("error"));
        this.readyState = 3;
        this.dispatchEvent(new Event("close"));
    }
    says(text: string) {
        this.dispatchEvent(new MessageEvent("message", { data: text }));
    }
    ends() {
        this.readyState = 3;
        this.dispatchEvent(new Event("close"));
    }
}

let sockets: FakeSocket[];
const signaling = () =>
    liaisonSignaling(LIAISON_URL, (url) => {
        const socket = new FakeSocket(url);
        sockets.push(socket);
        return socket as unknown as WebSocket;
    });

beforeEach(() => {
    sockets = [];
    vi.useFakeTimers();
});
afterEach(() => vi.useRealTimers());

describe("reading what liaison says", () => {
    it("reads every message liaison sends", () => {
        expect(parseServerMessage('{"type":"created","room":"K7Q-M4X"}')).toEqual({ type: "created", room: "K7Q-M4X" });
        expect(parseServerMessage('{"type":"joined"}')).toEqual({ type: "joined" });
        expect(parseServerMessage('{"type":"peer-joined"}')).toEqual({ type: "peer-joined" });
        expect(parseServerMessage('{"type":"peer-left"}')).toEqual({ type: "peer-left" });
        expect(parseServerMessage('{"type":"signal","data":{"offer":"sdp"}}')).toEqual({ type: "signal", data: { offer: "sdp" } });
        expect(parseServerMessage('{"type":"error","reason":"room-not-found"}')).toEqual({ type: "error", reason: "room-not-found" });
    });

    it("reads the host's build from an other-build error", () => {
        expect(parseServerMessage('{"type":"error","reason":"other-build","theirs":{"version":"1.3.0","commit":"b81e07d"}}')).toEqual({
            type: "error",
            reason: "other-build",
            theirs: { version: "1.3.0", commit: "b81e07d" },
        });
    });

    it("reads the relay liaison offers with created and joined", () => {
        const relay = { urls: ["turn:outpost.rmstdope.eu:3478?transport=udp", "turn:outpost.rmstdope.eu:3478?transport=tcp"], username: "u", credential: "c" };
        const servers = JSON.stringify([relay]);
        expect(parseServerMessage(`{"type":"created","room":"K7Q-M4X","iceServers":${servers}}`)).toEqual({
            type: "created",
            room: "K7Q-M4X",
            iceServers: [relay],
        });
        expect(parseServerMessage(`{"type":"joined","iceServers":${servers}}`)).toEqual({ type: "joined", iceServers: [relay] });
        expect(parseServerMessage('{"type":"joined","iceServers":[{"urls":"turns:r.example:443","username":"u","credential":"c"}]}')).toEqual({
            type: "joined",
            iceServers: [{ urls: "turns:r.example:443", username: "u", credential: "c" }],
        });
    });

    it("drops a relay that is not exactly TURN servers, and still reads the message", () => {
        for (const servers of [
            "7",
            "{}",
            "[]",
            '["turn:x"]',
            '[{"urls":["turn:x"]}]',
            '[{"urls":["turn:x"],"username":"u"}]',
            '[{"urls":["turn:x"],"username":"u","credential":5}]',
            '[{"urls":["stun:x"],"username":"u","credential":"c"}]',
            '[{"urls":["https://x"],"username":"u","credential":"c"}]',
            '[{"urls":[],"username":"u","credential":"c"}]',
            '[{"urls":[7],"username":"u","credential":"c"}]',
            JSON.stringify(Array.from({ length: 5 }, () => ({ urls: "turn:x", username: "u", credential: "c" }))),
            JSON.stringify([{ urls: Array.from({ length: 9 }, () => "turn:x"), username: "u", credential: "c" }]),
        ]) {
            expect(parseServerMessage(`{"type":"created","room":"K7Q-M4X","iceServers":${servers}}`), servers).toEqual({
                type: "created",
                room: "K7Q-M4X",
            });
            expect(parseServerMessage(`{"type":"joined","iceServers":${servers}}`), servers).toEqual({ type: "joined" });
        }
    });

    it("takes nothing that is not exactly a message", () => {
        for (const text of [
            "not json",
            "null",
            "[]",
            '{"type":"created"}',
            '{"type":"created","room":7}',
            '{"type":"error"}',
            '{"type":"error","reason":5}',
            '{"type":"error","reason":"other-build","theirs":{"version":1}}',
            '{"type":"signal"}',
            '{"type":"unknown"}',
        ]) {
            expect(parseServerMessage(text), text).toBeNull();
        }
    });
});

describe("the connection to liaison", () => {
    it("connects to liaison's address", async () => {
        const opening = signaling().open();
        expect(sockets[0].url).toBe("wss://liaison.rmstdope.eu/ws");
        sockets[0].opens();
        await expect(opening).resolves.toBeDefined();
    });

    it("cannot be opened when liaison cannot be reached", async () => {
        const opening = signaling().open();
        sockets[0].fails();
        await expect(opening).rejects.toThrow();
    });

    it("gives up when liaison does not answer within its limit", async () => {
        const opening = signaling().open();
        const failed = expect(opening).rejects.toThrow();
        vi.advanceTimersByTime(OPEN_LIMIT_MS);
        await failed;
        expect(sockets[0].closed).toBe(true);
    });

    async function opened(): Promise<SignalingSocket> {
        const opening = signaling().open();
        sockets[0].opens();
        return opening;
    }

    it("sends messages as JSON", async () => {
        const socket = await opened();
        socket.send({ type: "join", room: "K7Q-M4X", build: { version: "1.3.0", commit: "4f2a9c1" } });
        expect(JSON.parse(sockets[0].sent[0])).toEqual({ type: "join", room: "K7Q-M4X", build: { version: "1.3.0", commit: "4f2a9c1" } });
    });

    it("hands on what liaison says, and drops what it cannot read", async () => {
        const socket = await opened();
        const heard: unknown[] = [];
        socket.onMessage((message) => heard.push(message));
        sockets[0].says("garbage");
        sockets[0].says('{"type":"peer-joined"}');
        expect(heard).toEqual([{ type: "peer-joined" }]);
    });

    it("says when liaison ends the connection, but not when this side closes it", async () => {
        const first = await opened();
        const ended = vi.fn();
        first.onClosed(ended);
        sockets[0].ends();
        expect(ended).toHaveBeenCalledOnce();

        const opening = signaling().open();
        sockets[1].opens();
        const second = await opening;
        const notTold = vi.fn();
        second.onClosed(notTold);
        second.close();
        sockets[1].ends();
        expect(sockets[1].closed).toBe(true);
        expect(notTold).not.toHaveBeenCalled();
    });
});
