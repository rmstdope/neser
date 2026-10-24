import { describe, expect, it, vi } from "vitest";
import { wrapChannel } from "./peer_link";

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
