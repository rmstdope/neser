import { describe, expect, it } from "vitest";

import {
    type KeyBindingRow,
    type KeyBindingSink,
    applyKeyBindings,
    keyBindingsFor,
    parseKeyBindingTable,
} from "./key_bindings";

/** A few rows shaped as the wasm binding `key_binding_table()` returns them. */
const RAW = [
    { key: "w", console: "nes", input: "snesPad", player: 1, button: 4 },
    { key: "w", console: "nes", input: "pad", player: 1, button: 4 },
    { key: "o", console: "nes", input: "pad", player: 2, button: 0 },
    { key: "6", console: "nes", input: "vsCoin", player: 1, button: 0 },
    { key: "g", console: "gba", input: "pad", player: 1, button: 0 },
    { key: "4", console: "snes", input: "scopeTurbo", player: 1, button: 0 },
    { key: "4", console: "snes", input: "pad", player: 1, button: 2 },
    { key: "5", console: "snes", input: "scopePause", player: 1, button: 0 },
    { key: "5", console: "snes", input: "pad", player: 1, button: 3 },
];

const TABLE: KeyBindingRow[] = parseKeyBindingTable(RAW);

/** A sink that records what reached the console. */
function recorder(opts: { snesPadAccepts?: boolean; scopePlugged?: boolean } = {}) {
    const calls: string[] = [];
    const sink: KeyBindingSink = {
        pad: (port, button, pressed) => { calls.push(`pad ${port} ${button} ${pressed}`); },
        snesPad: (port, button, pressed) => {
            calls.push(`snesPad ${port} ${button} ${pressed}`);
            return opts.snesPadAccepts ?? false;
        },
        vsCoin: () => { calls.push("coin"); },
        scope: (action, pressed, repeat) => {
            if (!opts.scopePlugged) {
                return false;
            }
            calls.push(`scope ${action} ${pressed} ${repeat}`);
            return true;
        },
    };
    return { calls, sink };
}

function press(console: string, key: string, ports: number[], sink: KeyBindingSink, pressed = true, repeat = false) {
    return applyKeyBindings(keyBindingsFor(TABLE, console, key), sink, ports, { repeat }, pressed);
}

describe("parseKeyBindingTable", () => {
    it("keeps every row and its order", () => {
        expect(TABLE).toHaveLength(RAW.length);
        expect(keyBindingsFor(TABLE, "nes", "w").map((row) => row.input)).toEqual(["snesPad", "pad"]);
    });

    it("refuses an input the page cannot apply, so no binding is silently dropped", () => {
        expect(() => parseKeyBindingTable([{ key: "-", console: "nes", input: "vsService", player: 1, button: 0 }]))
            .toThrow(/vsService/);
    });
});

describe("keyBindingsFor", () => {
    it("matches the key case-insensitively, as typed with Shift", () => {
        expect(keyBindingsFor(TABLE, "nes", "W")).toHaveLength(2);
    });

    it("has nothing for an unbound key or another console", () => {
        expect(keyBindingsFor(TABLE, "nes", "x")).toEqual([]);
        expect(keyBindingsFor(TABLE, "gb", "w")).toEqual([]);
    });
});

describe("applyKeyBindings", () => {
    it("falls through to the joypad when no SNES pad takes the key", () => {
        const { calls, sink } = recorder();
        expect(press("nes", "w", [1, 2], sink)).toBe(true);
        expect(calls).toEqual(["snesPad 1 4 true", "pad 1 4 true"]);
    });

    it("stops at the SNES pad when one is plugged in", () => {
        const { calls, sink } = recorder({ snesPadAccepts: true });
        press("nes", "w", [1, 2], sink);
        expect(calls).toEqual(["snesPad 1 4 true"]);
    });

    it("routes player 2 keys to the second keyboard port", () => {
        const { calls, sink } = recorder();
        press("nes", "o", [3, 4], sink, false);
        expect(calls).toEqual(["pad 4 0 false"]);
    });

    it("does nothing, and is not handled, for a player with no keyboard port", () => {
        const { calls, sink } = recorder();
        expect(press("nes", "o", [], sink)).toBe(false);
        expect(press("nes", "w", [], sink)).toBe(false);
        expect(calls).toEqual([]);
    });

    it("inserts one Vs. coin on the press, whatever the ports, and none on repeat or release (nr-use)", () => {
        const { calls, sink } = recorder();
        expect(press("nes", "6", [], sink)).toBe(true);
        expect(press("nes", "6", [], sink, true, true)).toBe(true);
        expect(press("nes", "6", [], sink, false)).toBe(true);
        expect(calls).toEqual(["coin"]);
    });

    it("gives 4 and 5 to a plugged Super Scope before Select and Start", () => {
        const { calls, sink } = recorder({ scopePlugged: true });
        press("snes", "4", [1, 2], sink, true, false);
        press("snes", "5", [1, 2], sink, false);
        expect(calls).toEqual(["scope turbo true false", "scope pause false false"]);
    });

    it("keeps 4 and 5 as Select and Start without a scope", () => {
        const { calls, sink } = recorder();
        press("snes", "4", [1, 2], sink);
        press("snes", "5", [1, 2], sink);
        expect(calls).toEqual(["pad 1 2 true", "pad 1 3 true"]);
    });

    it("is not handled for a key with no rows", () => {
        const { calls, sink } = recorder();
        expect(press("gba", "q", [1], sink)).toBe(false);
        expect(calls).toEqual([]);
    });
});
