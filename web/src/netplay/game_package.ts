/**
 * What the host sends the joiner so both can play its game (nr-ayk5.4): the ROM, the firmware a
 * SNES coprocessor game needs, and a save state of the host's moment. One byte string, read back
 * on the joiner's side as untrusted: anything that does not describe itself exactly is refused,
 * never thrown over.
 *
 * Layout: "NSRG", a version byte, the header's length (u32, little-endian), the header as UTF-8
 * JSON `{kind, name, rom, chip, firmware, state}` (the three blob lengths and the chip key, or
 * null), then the ROM, the firmware and the state, back to back.
 */

import type { ConsoleKind } from "../console/consoles";

/** The consoles a network game can be played on. */
export const NETPLAY_KINDS = ["nes", "snes"] as const satisfies readonly ConsoleKind[];
export type NetplayKind = (typeof NETPLAY_KINDS)[number];

export type GamePackage = {
    kind: NetplayKind;
    name: string;
    rom: Uint8Array;
    firmware: { chip: string; image: Uint8Array } | null;
    state: Uint8Array;
};

/** The largest package taken from the other side; the largest SNES ROMs are 6 MB. */
export const MAX_PACKAGE_BYTES = 16 * 1024 * 1024;
const MAX_NAME_LENGTH = 255;
const MAGIC = [0x4e, 0x53, 0x52, 0x47]; // "NSRG"
const VERSION = 1;
const PREFIX = MAGIC.length + 1 + 4;

type Header = { kind: string; name: string; rom: number; chip: string | null; firmware: number; state: number };

export function packGame(game: GamePackage): Uint8Array {
    const firmware = game.firmware?.image ?? new Uint8Array();
    const header: Header = {
        kind: game.kind,
        name: game.name,
        rom: game.rom.length,
        chip: game.firmware?.chip ?? null,
        firmware: firmware.length,
        state: game.state.length,
    };
    const json = new TextEncoder().encode(JSON.stringify(header));
    const out = new Uint8Array(PREFIX + json.length + game.rom.length + firmware.length + game.state.length);
    out.set(MAGIC, 0);
    out[MAGIC.length] = VERSION;
    new DataView(out.buffer).setUint32(MAGIC.length + 1, json.length, true);
    let at = PREFIX;
    for (const part of [json, game.rom, firmware, game.state]) {
        out.set(part, at);
        at += part.length;
    }
    return out;
}

const isLength = (value: unknown): value is number => typeof value === "number" && Number.isInteger(value) && value >= 0;

export function unpackGame(bytes: Uint8Array): GamePackage | null {
    if (bytes.length > MAX_PACKAGE_BYTES || bytes.length < PREFIX) return null;
    if (MAGIC.some((value, i) => bytes[i] !== value) || bytes[MAGIC.length] !== VERSION) return null;
    const headerLength = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(MAGIC.length + 1, true);
    if (headerLength > bytes.length - PREFIX) return null;
    let header: Partial<Header>;
    try {
        header = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes.subarray(PREFIX, PREFIX + headerLength)));
    } catch {
        return null;
    }
    if (typeof header !== "object" || header === null) return null;
    const { kind, name, rom, chip, firmware, state } = header;
    if (!NETPLAY_KINDS.some((k) => k === kind)) return null;
    if (typeof name !== "string" || name.length === 0 || name.length > MAX_NAME_LENGTH) return null;
    if (!isLength(rom) || !isLength(firmware) || !isLength(state) || rom === 0 || state === 0) return null;
    if (chip !== null && typeof chip !== "string") return null;
    if ((chip === null) !== (firmware === 0)) return null;
    const start = PREFIX + headerLength;
    if (start + rom + firmware + state !== bytes.length) return null;
    const romBytes = bytes.slice(start, start + rom);
    const image = bytes.slice(start + rom, start + rom + firmware);
    const stateBytes = bytes.slice(start + rom + firmware);
    return { kind: kind as NetplayKind, name, rom: romBytes, firmware: chip === null ? null : { chip, image }, state: stateBytes };
}

let crcTable: Uint32Array | null = null;

/** CRC-32 (IEEE), the check that what arrived is what was sent; `crypto.subtle` is absent on a plain-http page. */
export function crc32(bytes: Uint8Array): number {
    if (!crcTable) {
        crcTable = new Uint32Array(256);
        for (let n = 0; n < 256; n++) {
            let c = n;
            for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
            crcTable[n] = c >>> 0;
        }
    }
    let crc = 0xffffffff;
    for (let i = 0; i < bytes.length; i++) crc = crcTable[(crc ^ bytes[i]) & 0xff] ^ (crc >>> 8);
    return (crc ^ 0xffffffff) >>> 0;
}
