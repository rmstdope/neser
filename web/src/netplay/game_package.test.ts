import { describe, expect, it } from "vitest";
import { crc32, MAX_PACKAGE_BYTES, packGame, unpackGame, type GamePackage } from "./game_package";

const bytes = (...values: number[]) => new Uint8Array(values);

const NES: GamePackage = { kind: "nes", name: "Super Mario Bros. 3.nes", rom: bytes(0x4e, 0x45, 0x53, 0x1a, 1, 2), firmware: null, state: bytes(7, 7, 7) };
const SNES: GamePackage = {
    kind: "snes",
    name: "Pilotwings.sfc",
    rom: new Uint8Array(4096).fill(0xaa),
    firmware: { chip: "dsp1", image: new Uint8Array(8192).fill(0x55) },
    state: bytes(1, 2, 3, 4),
};

/** The header's JSON, rewritten, with the blobs kept: for building packages that lie about themselves. */
function withHeader(game: GamePackage, edit: (header: Record<string, unknown>) => void): Uint8Array {
    const packed = packGame(game);
    const view = new DataView(packed.buffer, packed.byteOffset);
    const length = view.getUint32(5, true);
    const header = JSON.parse(new TextDecoder().decode(packed.subarray(9, 9 + length)));
    edit(header);
    const json = new TextEncoder().encode(JSON.stringify(header));
    const out = new Uint8Array(9 + json.length + packed.length - 9 - length);
    out.set(packed.subarray(0, 9));
    new DataView(out.buffer).setUint32(5, json.length, true);
    out.set(json, 9);
    out.set(packed.subarray(9 + length), 9 + json.length);
    return out;
}

describe("the game package", () => {
    it("carries a NES game and its moment unchanged", () => {
        expect(unpackGame(packGame(NES))).toEqual(NES);
    });

    it("carries a SNES game with the firmware its chip needs", () => {
        expect(unpackGame(packGame(SNES))).toEqual(SNES);
    });

    it("is not a package without its magic, or of another version", () => {
        const packed = packGame(NES);
        expect(unpackGame(new Uint8Array([0x58, ...packed.subarray(1)]))).toBeNull();
        const other = packed.slice();
        other[4] = 2;
        expect(unpackGame(other)).toBeNull();
    });

    it("is refused when its header is not JSON, or names an unknown console", () => {
        const packed = packGame(NES);
        const broken = packed.slice();
        broken[9] = 0x7b + 1;
        expect(unpackGame(broken)).toBeNull();
        expect(unpackGame(withHeader(NES, (h) => (h.kind = "gba")))).toBeNull();
    });

    it("is refused when the lengths it states do not add up to what came", () => {
        expect(unpackGame(withHeader(NES, (h) => (h.rom = (h.rom as number) + 1)))).toBeNull();
        expect(unpackGame(withHeader(NES, (h) => (h.state = -1)))).toBeNull();
        expect(unpackGame(withHeader(NES, (h) => (h.state = 1.5)))).toBeNull();
        expect(unpackGame(packGame(NES).subarray(0, 20))).toBeNull();
        expect(unpackGame(new Uint8Array([...packGame(NES), 0]))).toBeNull();
    });

    it("is refused without a ROM or a moment to start from", () => {
        expect(unpackGame(packGame({ ...NES, rom: bytes() }))).toBeNull();
        expect(unpackGame(packGame({ ...NES, state: bytes() }))).toBeNull();
    });

    it("is refused with a name that is not a short string", () => {
        expect(unpackGame(withHeader(NES, (h) => (h.name = 42)))).toBeNull();
        expect(unpackGame(withHeader(NES, (h) => (h.name = "x".repeat(256))))).toBeNull();
        expect(unpackGame(withHeader(NES, (h) => (h.name = "")))).toBeNull();
    });

    it("is refused with a firmware chip that is not a string", () => {
        expect(unpackGame(withHeader(SNES, (h) => (h.chip = 3)))).toBeNull();
        expect(unpackGame(withHeader(NES, (h) => (h.firmware = 4)))).toBeNull();
    });

    it("is refused above the largest size accepted", () => {
        expect(unpackGame(new Uint8Array(MAX_PACKAGE_BYTES + 1))).toBeNull();
    });

    it("never throws on random or cut-off bytes", () => {
        const packed = packGame(SNES);
        let seed = 12345;
        const random = () => (seed = (seed * 1103515245 + 12345) >>> 0) & 0xff;
        for (let i = 0; i < 500; i++) {
            const cut = packed.subarray(0, random() * 50);
            const noise = new Uint8Array(random()).map(() => random());
            const flipped = packed.slice();
            flipped[random() % 40] ^= 0xff;
            for (const candidate of [cut, noise, flipped]) expect(() => unpackGame(candidate)).not.toThrow();
        }
    });
});

describe("crc32", () => {
    it("is the standard CRC-32", () => {
        expect(crc32(new TextEncoder().encode("123456789"))).toBe(0xcbf43926);
        expect(crc32(bytes())).toBe(0);
    });
});
