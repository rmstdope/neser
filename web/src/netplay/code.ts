import { sameBuild, type BuildId } from "./build_id";

/**
 * The codes two players copy to each other to connect (nr-ayk5.3). The host's code carries its
 * WebRTC offer and the joiner's reply code its answer, each with every network address the
 * browser found, so no server is needed. A code is `NSR1-` and the base64url of the deflated JSON
 * `{k, v, c, s}`: its kind, NESER version, commit and SDP.
 */
export type CodeKind = "host" | "reply";

export type CodeCheck =
    | { ok: true; sdp: string }
    | { ok: false; problem: "not-neser" }
    | { ok: false; problem: "wrong-kind" }
    | { ok: false; problem: "other-build"; theirs: BuildId };

const PREFIX = "NSR1-";

type Payload = { k: CodeKind; v: string; c: string; s: string };

async function pipe(bytes: Uint8Array, stream: CompressionStream | DecompressionStream): Promise<Uint8Array> {
    const out = new Blob([bytes as BlobPart]).stream().pipeThrough(stream);
    return new Uint8Array(await new Response(out).arrayBuffer());
}

function toBase64Url(bytes: Uint8Array): string {
    let binary = "";
    for (const byte of bytes) binary += String.fromCharCode(byte);
    return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function fromBase64Url(text: string): Uint8Array {
    const binary = atob(text.replace(/-/g, "+").replace(/_/g, "/"));
    return Uint8Array.from(binary, (char) => char.charCodeAt(0));
}

export async function encodeCode(kind: CodeKind, build: BuildId, sdp: string): Promise<string> {
    const payload: Payload = { k: kind, v: build.version, c: build.commit, s: sdp };
    const packed = await pipe(new TextEncoder().encode(JSON.stringify(payload)), new CompressionStream("deflate-raw"));
    return PREFIX + toBase64Url(packed);
}

async function decode(text: string): Promise<Payload | null> {
    const code = text.replace(/\s+/g, "");
    if (!code.startsWith(PREFIX)) return null;
    try {
        const json = new TextDecoder().decode(await pipe(fromBase64Url(code.slice(PREFIX.length)), new DecompressionStream("deflate-raw")));
        const payload = JSON.parse(json) as Partial<Payload>;
        const fields = [payload.v, payload.c, payload.s].every((field) => typeof field === "string");
        return fields && (payload.k === "host" || payload.k === "reply") ? (payload as Payload) : null;
    } catch {
        return null;
    }
}

/**
 * What a pasted code is, checked in the order a player fixes things: is it a NESER code at all,
 * is it the kind this step wants, and was it made by the same NESER build as ours.
 */
export async function readCode(text: string, expected: CodeKind, own: BuildId): Promise<CodeCheck> {
    const payload = await decode(text);
    if (!payload) return { ok: false, problem: "not-neser" };
    if (payload.k !== expected) return { ok: false, problem: "wrong-kind" };
    const theirs = { version: payload.v, commit: payload.c };
    if (!sameBuild(own, theirs)) return { ok: false, problem: "other-build", theirs };
    return { ok: true, sdp: payload.s };
}
