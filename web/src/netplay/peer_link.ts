/**
 * The direct connection between two browsers (nr-ayk5.3): one WebRTC data channel, set up from
 * the offer and answer the two players copy to each other. No STUN or TURN server is asked, so
 * only the browsers' own addresses are offered and the two must reach each other directly. This
 * is the only file that touches `RTCPeerConnection`; the session drives it through `PeerLinks`.
 */

/** An open channel to the other browser. `onClosed` is told once, however the channel ends. */
export interface PeerLink {
    send(text: string): void;
    onMessage(listener: (text: string) => void): void;
    onClosed(listener: () => void): void;
    close(): void;
}

/** A host's offer, ready to be sent; `accept` takes the joiner's answer and opens the channel. */
export interface HostStart {
    sdp: string;
    accept(replySdp: string): Promise<PeerLink>;
    cancel(): void;
}

/** A joiner's answer to a host's offer, ready to be sent back; `link` opens once the host accepts it. */
export interface JoinStart {
    sdp: string;
    link: Promise<PeerLink>;
    cancel(): void;
}

export interface PeerLinks {
    startHost(): Promise<HostStart>;
    startJoin(hostSdp: string): Promise<JoinStart>;
}

/** A code carries every address at once, so the offer and answer wait for the browser to find them. */
const GATHER_LIMIT_MS = 3000;

function gathered(pc: RTCPeerConnection): Promise<void> {
    return new Promise((resolve) => {
        if (pc.iceGatheringState === "complete") return resolve();
        const timer = setTimeout(resolve, GATHER_LIMIT_MS);
        pc.addEventListener("icegatheringstatechange", () => {
            if (pc.iceGatheringState !== "complete") return;
            clearTimeout(timer);
            resolve();
        });
    });
}

function wrap(pc: RTCPeerConnection, channel: RTCDataChannel): PeerLink {
    let closedListener: () => void = () => {};
    let ended = false;
    const end = () => {
        if (ended) return;
        ended = true;
        closedListener();
    };
    channel.addEventListener("close", end);
    pc.addEventListener("connectionstatechange", () => {
        if (pc.connectionState === "failed" || pc.connectionState === "closed") end();
    });
    return {
        send: (text) => {
            if (channel.readyState === "open") channel.send(text);
        },
        onMessage: (listener) => channel.addEventListener("message", (event) => listener(String(event.data))),
        onClosed: (listener) => {
            closedListener = listener;
        },
        close: () => {
            ended = true;
            channel.close();
            pc.close();
        },
    };
}

/** Resolves once `channel` is open, or fails when the two browsers cannot reach each other. */
function opened(pc: RTCPeerConnection, channel: Promise<RTCDataChannel>): Promise<PeerLink> {
    return new Promise((resolve, reject) => {
        pc.addEventListener("connectionstatechange", () => {
            if (pc.connectionState === "failed" || pc.connectionState === "closed") reject(new Error("unreachable"));
        });
        channel.then((ch) => {
            if (ch.readyState === "open") resolve(wrap(pc, ch));
            else ch.addEventListener("open", () => resolve(wrap(pc, ch)), { once: true });
        });
    });
}

async function startHost(): Promise<HostStart> {
    const pc = new RTCPeerConnection({ iceServers: [] });
    const channel = pc.createDataChannel("neser");
    await pc.setLocalDescription();
    await gathered(pc);
    return {
        sdp: pc.localDescription!.sdp,
        accept: async (replySdp) => {
            await pc.setRemoteDescription({ type: "answer", sdp: replySdp });
            return opened(pc, Promise.resolve(channel));
        },
        cancel: () => pc.close(),
    };
}

async function startJoin(hostSdp: string): Promise<JoinStart> {
    const pc = new RTCPeerConnection({ iceServers: [] });
    const channel = new Promise<RTCDataChannel>((resolve) => {
        pc.addEventListener("datachannel", (event) => resolve(event.channel), { once: true });
    });
    const link = opened(pc, channel);
    // A joiner may wait long for the host; a failure before then is reported through `link`.
    link.catch(() => {});
    await pc.setRemoteDescription({ type: "offer", sdp: hostSdp });
    await pc.setLocalDescription();
    await gathered(pc);
    return { sdp: pc.localDescription!.sdp, link, cancel: () => pc.close() };
}

export const webRtcLinks: PeerLinks = { startHost, startJoin };
