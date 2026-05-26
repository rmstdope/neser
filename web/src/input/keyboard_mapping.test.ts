import { expect, it } from "vitest";

import { gbaKeyboardButtonForEvent } from "./keyboard_mapping";

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
