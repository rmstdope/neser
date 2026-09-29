import { describe, expect, it } from "vitest";

import {
    applyVsCoinKey,
    gbaKeyboardButtonForEvent,
    snesKeyboardButtonForEvent
} from "./keyboard_mapping";

function event(key: string, code = key): Pick<KeyboardEvent, "key" | "code"> {
    return { key, code };
}

it("maps GBA keyboard face buttons", () => {
    expect(gbaKeyboardButtonForEvent(event("g", "KeyG"))).toBe(0);
    expect(gbaKeyboardButtonForEvent(event("f", "KeyF"))).toBe(1);
});

it("maps GBA keyboard system buttons", () => {
    expect(gbaKeyboardButtonForEvent(event("5", "Digit5"))).toBe(3);
    expect(gbaKeyboardButtonForEvent(event("4", "Digit4"))).toBe(2);
});

it("maps GBA keyboard d-pad buttons", () => {
    expect(gbaKeyboardButtonForEvent(event("w", "KeyW"))).toBe(4);
    expect(gbaKeyboardButtonForEvent(event("s", "KeyS"))).toBe(5);
    expect(gbaKeyboardButtonForEvent(event("a", "KeyA"))).toBe(6);
    expect(gbaKeyboardButtonForEvent(event("d", "KeyD"))).toBe(7);
});

it("maps GBA keyboard shoulder buttons", () => {
    expect(gbaKeyboardButtonForEvent(event("v", "KeyV"))).toBe(8);
    expect(gbaKeyboardButtonForEvent(event("b", "KeyB"))).toBe(9);
});

it("ignores keys outside the GBA keyboard mapping", () => {
    expect(gbaKeyboardButtonForEvent(event("q", "KeyQ"))).toBeNull();
    expect(gbaKeyboardButtonForEvent(event(" ", "Space"))).toBeNull();
});

it("maps SNES keyboard face and shoulder buttons", () => {
    expect(snesKeyboardButtonForEvent(event("r", "KeyR"))).toBe(0); // B
    expect(snesKeyboardButtonForEvent(event("t", "KeyT"))).toBe(8); // A
    expect(snesKeyboardButtonForEvent(event("y", "KeyY"))).toBe(9); // X
    expect(snesKeyboardButtonForEvent(event("g", "KeyG"))).toBe(1); // Y
    expect(snesKeyboardButtonForEvent(event("q", "KeyQ"))).toBe(10); // L
    expect(snesKeyboardButtonForEvent(event("e", "KeyE"))).toBe(11); // R
});

it("maps SNES keyboard system and d-pad buttons", () => {
    expect(snesKeyboardButtonForEvent(event("4", "Digit4"))).toBe(2); // Select
    expect(snesKeyboardButtonForEvent(event("5", "Digit5"))).toBe(3); // Start
    expect(snesKeyboardButtonForEvent(event("w", "KeyW"))).toBe(4);
    expect(snesKeyboardButtonForEvent(event("s", "KeyS"))).toBe(5);
    expect(snesKeyboardButtonForEvent(event("a", "KeyA"))).toBe(6);
    expect(snesKeyboardButtonForEvent(event("d", "KeyD"))).toBe(7);
});

it("ignores unmapped keys for SNES keyboard mapping", () => {
    expect(snesKeyboardButtonForEvent(event("f", "KeyF"))).toBeNull();
    expect(snesKeyboardButtonForEvent(event(" ", "Space"))).toBeNull();
});

describe("Vs. System coin key (nr-use)", () => {
    function coinTarget() {
        const slots: number[] = [];
        return { slots, insert_vs_coin: (slot: number) => { slots.push(slot); } };
    }

    function key(k: string, repeat = false): Pick<KeyboardEvent, "key" | "repeat"> {
        return { key: k, repeat };
    }

    it("inserts one coin into slot 1 when 6 is pressed, as on desktop", () => {
        const nes = coinTarget();
        expect(applyVsCoinKey(nes, key("6"), true)).toBe(true);
        expect(nes.slots).toEqual([0]);
    });

    it("inserts no further coin while 6 is held and repeats, nor on release", () => {
        const nes = coinTarget();
        applyVsCoinKey(nes, key("6"), true);
        expect(applyVsCoinKey(nes, key("6", true), true)).toBe(true);
        expect(applyVsCoinKey(nes, key("6"), false)).toBe(true);
        expect(nes.slots).toEqual([0]);
    });

    it("leaves every other key alone", () => {
        const nes = coinTarget();
        expect(applyVsCoinKey(nes, key("5"), true)).toBe(false);
        expect(nes.slots).toEqual([]);
    });
});
