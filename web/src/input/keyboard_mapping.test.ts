import { expect, it } from "vitest";

import { gbaKeyboardButtonForEvent } from "./keyboard_mapping";

function event(key: string, code = key): Pick<KeyboardEvent, "key" | "code"> {
    return { key, code };
}

it("maps GBA keyboard face buttons and shoulders", () => {
    expect(gbaKeyboardButtonForEvent(event("z", "KeyZ"))).toBe(0);
    expect(gbaKeyboardButtonForEvent(event("x", "KeyX"))).toBe(1);
    expect(gbaKeyboardButtonForEvent(event("a", "KeyA"))).toBe(8);
    expect(gbaKeyboardButtonForEvent(event("s", "KeyS"))).toBe(9);
});

it("maps GBA keyboard system buttons", () => {
    expect(gbaKeyboardButtonForEvent(event("5", "Digit5"))).toBe(3);
    expect(gbaKeyboardButtonForEvent(event("Enter", "Enter"))).toBe(3);
    expect(gbaKeyboardButtonForEvent(event("Shift", "ShiftLeft"))).toBe(2);
    expect(gbaKeyboardButtonForEvent(event("Shift", "ShiftRight"))).toBe(2);
    expect(gbaKeyboardButtonForEvent(event("Backspace", "Backspace"))).toBe(2);
});

it("maps GBA keyboard d-pad buttons", () => {
    expect(gbaKeyboardButtonForEvent(event("ArrowUp", "ArrowUp"))).toBe(4);
    expect(gbaKeyboardButtonForEvent(event("ArrowDown", "ArrowDown"))).toBe(5);
    expect(gbaKeyboardButtonForEvent(event("v", "KeyV"))).toBe(6);
    expect(gbaKeyboardButtonForEvent(event("b", "KeyB"))).toBe(7);
    expect(gbaKeyboardButtonForEvent(event("ArrowLeft", "ArrowLeft"))).toBe(6);
    expect(gbaKeyboardButtonForEvent(event("ArrowRight", "ArrowRight"))).toBe(7);
});

it("ignores keys outside the GBA keyboard mapping", () => {
    expect(gbaKeyboardButtonForEvent(event("q", "KeyQ"))).toBeNull();
    expect(gbaKeyboardButtonForEvent(event(" ", "Space"))).toBeNull();
});
