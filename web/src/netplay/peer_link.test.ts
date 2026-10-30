import { describe, expect, it, vi } from "vitest";
import { isRelayed, webRtcLinks, wrapChannel } from "./peer_link";

/** Just enough of a data channel and its connection for the link wrapper. */
function fakeChannel() {
    const channel = Object.assign(new EventTarget(), {
        readyState: "open" as RTCDataChannelState,
        binaryType: "blob" as BinaryType,
        send: vi.fn(),
        close: vi.fn(),
    });
    const pc = Object.assign(new EventTarget(), { connectionState: "connected", close: vi.fn() });
    const link = wrapChannel(pc as unknown as RTCPeerConnection, channel as unknown as RTCDataChannel);
    const receive = (data: unknown) => channel.dispatchEvent(Object.assign(new Event("message"), { data }));
    return { channel, link, receive };
}

describe("the link to the other browser", () => {
    it("receives binary messages as bytes", () => {
        const { channel, link, receive } = fakeChannel();
        expect(channel.binaryType).toBe("arraybuffer");
        const got: (string | Uint8Array)[] = [];
        link.onMessage((data) => got.push(data));
        receive(new Uint8Array([1, 2, 3]).buffer);
        receive("hello");
        expect(got[0]).toBeInstanceOf(Uint8Array);
        expect(Array.from(got[0] as Uint8Array)).toEqual([1, 2, 3]);
        expect(got[1]).toBe("hello");
    });

    it("keeps what arrives before anyone listens, for the first listener (nr-1wbx: the link opens after reading its stats)", () => {
        const { link, receive } = fakeChannel();
        receive("ping");
        receive(new Uint8Array([4]).buffer);
        const got: (string | Uint8Array)[] = [];
        link.onMessage((data) => got.push(data));
        receive("later");
        expect(got.map((data) => (typeof data === "string" ? data : Array.from(data)))).toEqual(["ping", [4], "later"]);
    });

    it("tells a listener that comes late that the channel has already closed", () => {
        const { channel, link } = fakeChannel();
        channel.dispatchEvent(new Event("close"));
        const closed = vi.fn();
        link.onClosed(closed);
        expect(closed).toHaveBeenCalledOnce();
    });

    it("does not tell of a close this side made", () => {
        const { link } = fakeChannel();
        link.close();
        const closed = vi.fn();
        link.onClosed(closed);
        expect(closed).not.toHaveBeenCalled();
    });

    it("sends bytes as bytes and text as text", () => {
        const { channel, link } = fakeChannel();
        const bytes = new Uint8Array([9, 8]);
        link.send(bytes);
        link.send("hi");
        expect(channel.send).toHaveBeenNthCalledWith(1, bytes);
        expect(channel.send).toHaveBeenNthCalledWith(2, "hi");
    });
});

const RELAY: RTCIceServer = { urls: ["turn:outpost.rmstdope.eu:3478?transport=udp"], username: "u", credential: "c" };

/** Just enough of a connection to start either side; `stats` is what `getStats` reports. */
function fakeConnections(stats: () => Promise<Map<string, Record<string, unknown>>> = async () => new Map()) {
    const configs: RTCConfiguration[] = [];
    const made: EventTarget[] = [];
    class FakeConnection extends EventTarget {
        iceGatheringState = "complete";
        connectionState = "new";
        localDescription: { sdp: string } | null = null;
        constructor(config: RTCConfiguration) {
            super();
            configs.push(config);
            made.push(this);
        }
        createDataChannel() {
            return new EventTarget();
        }
        async setLocalDescription() {
            this.localDescription = { sdp: "local" };
        }
        async setRemoteDescription() {}
        getStats() {
            return stats();
        }
        close() {}
    }
    return { FakeConnection, configs, made };
}

/** The selected pair between a local and a remote candidate of these types. */
function pairStats(local: string, remote: string, how: "transport" | "nominated" = "transport") {
    const report = new Map<string, Record<string, unknown>>();
    report.set("L", { id: "L", type: "local-candidate", candidateType: local });
    report.set("R", { id: "R", type: "remote-candidate", candidateType: remote });
    report.set("P0", { id: "P0", type: "candidate-pair", localCandidateId: "R", remoteCandidateId: "R", state: "failed" });
    report.set("P", { id: "P", type: "candidate-pair", localCandidateId: "L", remoteCandidateId: "R", state: "succeeded", nominated: how === "nominated" });
    if (how === "transport") report.set("T", { id: "T", type: "transport", selectedCandidatePairId: "P" });
    return report;
}

describe("connecting across the internet", () => {
    it("asks outpost, NESER's own STUN server, for each browser's public address", async () => {
        const { FakeConnection, configs } = fakeConnections();
        vi.stubGlobal("RTCPeerConnection", FakeConnection);
        try {
            (await webRtcLinks.startHost([])).cancel();
            (await webRtcLinks.startJoin("offer", [])).cancel();
        } finally {
            vi.unstubAllGlobals();
        }
        const outpost = { iceServers: [{ urls: "stun:outpost.rmstdope.eu:3478" }] };
        expect(configs).toEqual([outpost, outpost]);
    });

    it("offers the room's relay beside outpost, for when no direct path works", async () => {
        const { FakeConnection, configs } = fakeConnections();
        vi.stubGlobal("RTCPeerConnection", FakeConnection);
        try {
            (await webRtcLinks.startHost([RELAY])).cancel();
            (await webRtcLinks.startJoin("offer", [RELAY])).cancel();
        } finally {
            vi.unstubAllGlobals();
        }
        const both = { iceServers: [{ urls: "stun:outpost.rmstdope.eu:3478" }, RELAY] };
        expect(configs).toEqual([both, both]);
    });
});

describe("telling a relayed connection from a direct one", () => {
    it("is relayed when either end of the chosen pair is the relay", () => {
        expect(isRelayed(pairStats("relay", "srflx") as unknown as RTCStatsReport)).toBe(true);
        expect(isRelayed(pairStats("host", "relay") as unknown as RTCStatsReport)).toBe(true);
        expect(isRelayed(pairStats("relay", "relay", "nominated") as unknown as RTCStatsReport)).toBe(true);
    });

    it("is direct when neither end is", () => {
        expect(isRelayed(pairStats("host", "host") as unknown as RTCStatsReport)).toBe(false);
        expect(isRelayed(pairStats("srflx", "prflx", "nominated") as unknown as RTCStatsReport)).toBe(false);
        expect(isRelayed(new Map() as unknown as RTCStatsReport)).toBe(false);
    });

    it("says so on the link once the channel opens", async () => {
        const { FakeConnection, made } = fakeConnections(async () => pairStats("relay", "srflx"));
        vi.stubGlobal("RTCPeerConnection", FakeConnection);
        try {
            const start = await webRtcLinks.startJoin("offer", [RELAY]);
            const channel = Object.assign(new EventTarget(), { readyState: "open", binaryType: "blob", close: vi.fn() });
            made[0].dispatchEvent(Object.assign(new Event("datachannel"), { channel }));
            expect((await start.link).relayed).toBe(true);
        } finally {
            vi.unstubAllGlobals();
        }
    });

    it("counts a connection whose stats cannot be read as direct", async () => {
        const { FakeConnection, made } = fakeConnections(() => Promise.reject(new Error("closed")));
        vi.stubGlobal("RTCPeerConnection", FakeConnection);
        try {
            const start = await webRtcLinks.startJoin("offer", []);
            const channel = Object.assign(new EventTarget(), { readyState: "open", binaryType: "blob", close: vi.fn() });
            made[0].dispatchEvent(Object.assign(new Event("datachannel"), { channel }));
            expect((await start.link).relayed).toBe(false);
        } finally {
            vi.unstubAllGlobals();
        }
    });
});
