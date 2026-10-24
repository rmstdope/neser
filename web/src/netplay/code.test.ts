import { describe, expect, it } from "vitest";
import { encodeCode, readCode } from "./code";

const OURS = { version: "1.3.0", commit: "4f2a9c1" };
const THEIRS = { version: "1.3.0", commit: "b81e07d" };
const SDP = "v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\ns=-\r\na=candidate:1 1 udp 2113937151 abc.local 55146 typ host\r\n".repeat(4);

describe("a network play code", () => {
    it("starts with NSR1- and is one unbroken line", async () => {
        const code = await encodeCode("host", OURS, SDP);
        expect(code.startsWith("NSR1-")).toBe(true);
        expect(code).toMatch(/^[A-Za-z0-9_\-]+$/);
    });

    it("gives back the host's connection details to a joiner on the same build", async () => {
        const code = await encodeCode("host", OURS, SDP);
        expect(await readCode(code, "host", OURS)).toEqual({ ok: true, sdp: SDP });
    });

    it("gives back the reply's connection details to the host", async () => {
        const code = await encodeCode("reply", OURS, SDP);
        expect(await readCode(code, "reply", OURS)).toEqual({ ok: true, sdp: SDP });
    });

    it("is read even when a chat app broke it over lines or padded it with spaces", async () => {
        const code = await encodeCode("host", OURS, SDP);
        const wrapped = `  ${code.slice(0, 40)}\n${code.slice(40, 90)}\r\n ${code.slice(90)}\n`;
        expect(await readCode(wrapped, "host", OURS)).toEqual({ ok: true, sdp: SDP });
    });

    it("is not a NESER code when it is ordinary text", async () => {
        expect(await readCode("hello can u paste the thing", "host", OURS)).toEqual({ ok: false, problem: "not-neser" });
        expect(await readCode("", "host", OURS)).toEqual({ ok: false, problem: "not-neser" });
    });

    it("is not a NESER code when only part of it was copied", async () => {
        const code = await encodeCode("host", OURS, SDP);
        expect(await readCode(code.slice(0, code.length - 30), "host", OURS)).toEqual({ ok: false, problem: "not-neser" });
    });

    it("is the wrong kind when a reply is pasted where the host's code belongs, and the other way round", async () => {
        const reply = await encodeCode("reply", OURS, SDP);
        const host = await encodeCode("host", OURS, SDP);
        expect(await readCode(reply, "host", OURS)).toEqual({ ok: false, problem: "wrong-kind" });
        expect(await readCode(host, "reply", OURS)).toEqual({ ok: false, problem: "wrong-kind" });
    });

    it("names the other player's build when it differs from ours", async () => {
        const code = await encodeCode("host", THEIRS, SDP);
        expect(await readCode(code, "host", OURS)).toEqual({ ok: false, problem: "other-build", theirs: THEIRS });
    });
});
