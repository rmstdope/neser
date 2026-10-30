import { describe, expect, it, vi } from "vitest";
import { webRtcLinks, wrapChannel } from "./peer_link";

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

    it("sends bytes as bytes and text as text", () => {
        const { channel, link } = fakeChannel();
        const bytes = new Uint8Array([9, 8]);
        link.send(bytes);
        link.send("hi");
        expect(channel.send).toHaveBeenNthCalledWith(1, bytes);
        expect(channel.send).toHaveBeenNthCalledWith(2, "hi");
    });
});

describe("connecting across the internet", () => {
    it("asks outpost, NESER's own STUN server, for each browser's public address", async () => {
        const configs: RTCConfiguration[] = [];
        class FakeConnection extends EventTarget {
            iceGatheringState = "complete";
            localDescription: { sdp: string } | null = null;
            constructor(config: RTCConfiguration) {
                super();
                configs.push(config);
            }
            createDataChannel() {
                return new EventTarget();
            }
            async setLocalDescription() {
                this.localDescription = { sdp: "local" };
            }
            async setRemoteDescription() {}
            close() {}
        }
        vi.stubGlobal("RTCPeerConnection", FakeConnection);
        try {
            (await webRtcLinks.startHost()).cancel();
            (await webRtcLinks.startJoin("offer")).cancel();
        } finally {
            vi.unstubAllGlobals();
        }
        const outpost = { iceServers: [{ urls: "stun:outpost.rmstdope.eu:3478" }] };
        expect(configs).toEqual([outpost, outpost]);
    });
});
