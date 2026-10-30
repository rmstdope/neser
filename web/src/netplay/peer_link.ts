/**
 * The direct connection between two browsers (nr-ayk5.3): one WebRTC data channel, set up from
 * an offer and an answer the two browsers swap through liaison (nr-z4nw). Each browser asks
 * outpost, NESER's own STUN server, for the public address its router gives it (nr-ri8n), so two
 * players in different homes can reach each other directly; no TURN relay is used (liaison's is
 * left for nr-1wbx), so a pair whose networks block a direct connection cannot connect. This is
 * the only file that touches `RTCPeerConnection`; the session drives it through `PeerLinks`.
 */

/** outpost answers STUN only; it never carries game data. Without it, same-network play still works. */
export const ICE_SERVERS: RTCIceServer[] = [{ urls: "stun:outpost.rmstdope.eu:3478" }];

/**
 * An open channel to the other browser, carrying text (the session's messages) and bytes (a game
 * being sent, nr-ayk5.4). `onClosed` is told once, however the channel ends.
 */
export interface PeerLink {
    send(data: string | Uint8Array): void;
    onMessage(listener: (data: string | Uint8Array) => void): void;
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

/**
 * The offer and answer each carry every address at once, so they wait for the browser to find them;
 * with outpost out of reach, the wait ends here and only the browser's own addresses are offered.
 */
const GATHER_LIMIT_MS = 3000;
/** How long a closing channel may take to send what is queued before the connection is ended anyway. */
const CLOSE_LIMIT_MS = 2000;

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

export function wrapChannel(pc: RTCPeerConnection, channel: RTCDataChannel): PeerLink {
    channel.binaryType = "arraybuffer";
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
        send: (data) => {
            if (channel.readyState !== "open") return;
            if (typeof data === "string") channel.send(data);
            else channel.send(data as Uint8Array<ArrayBuffer>);
        },
        onMessage: (listener) =>
            channel.addEventListener("message", (event) =>
                listener(event.data instanceof ArrayBuffer ? new Uint8Array(event.data) : String(event.data))
            ),
        onClosed: (listener) => {
            closedListener = listener;
        },
        // Closing the connection itself would abort the channel and drop what is still queued (a
        // refusal, a goodbye), so the channel closes first and takes the connection with it.
        close: () => {
            ended = true;
            if (channel.readyState === "closed") return pc.close();
            channel.addEventListener("close", () => pc.close(), { once: true });
            setTimeout(() => pc.close(), CLOSE_LIMIT_MS);
            channel.close();
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
            if (ch.readyState === "open") resolve(wrapChannel(pc, ch));
            else ch.addEventListener("open", () => resolve(wrapChannel(pc, ch)), { once: true });
        });
    });
}

async function startHost(): Promise<HostStart> {
    const pc = new RTCPeerConnection({ iceServers: ICE_SERVERS });
    const channel = pc.createDataChannel("neser");
    try {
        await pc.setLocalDescription();
        await gathered(pc);
    } catch (error) {
        pc.close();
        throw error;
    }
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
    const pc = new RTCPeerConnection({ iceServers: ICE_SERVERS });
    const channel = new Promise<RTCDataChannel>((resolve) => {
        pc.addEventListener("datachannel", (event) => resolve(event.channel), { once: true });
    });
    const link = opened(pc, channel);
    // A joiner may wait long for the host; a failure before then is reported through `link`.
    link.catch(() => {});
    try {
        await pc.setRemoteDescription({ type: "offer", sdp: hostSdp });
        await pc.setLocalDescription();
        await gathered(pc);
    } catch (error) {
        // A code whose connection details the browser refuses: nothing will use this connection.
        pc.close();
        throw error;
    }
    return { sdp: pc.localDescription!.sdp, link, cancel: () => pc.close() };
}

export const webRtcLinks: PeerLinks = { startHost, startJoin };
