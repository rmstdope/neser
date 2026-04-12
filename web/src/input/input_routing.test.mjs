import { expect, it } from "vitest";

import { selectGamepads } from "./gamepad.js";
import { 
    getKeyboardControllerTarget,
    shouldSuppressJoypadInput
} from "./input_routing.js";

function makeGamepad(index) {
    return {
        connected: true,
        index,
        buttons: Array.from({ length: 16 }, () => ({ pressed: false })),
        axes: [0, 0, 0, 0]
    };
}
    
function makeNesStub({ arkanoidPort = null } = {}) {
    return {
        is_mouse_emulated_controller: (controller) => (arkanoidPort === controller),
    };
}

// Tests for selectGamepads
it("selectGamepads returns empty array when no gamepads connected", () => {
    const gamepads = [null, { connected: false }, undefined];
    const selected = selectGamepads(gamepads);
    expect(selected).toEqual([]);
});

it("selectGamepads returns one gamepad when only one connected", () => {
    const gamepad1 = makeGamepad(1);
    const gamepads = [null, gamepad1, null];
    const selected = selectGamepads(gamepads);
    
    expect(selected.length).toBe(1);
    expect(selected[0]).toBe(gamepad1);
});

it("selectGamepads returns two gamepads when two connected", () => {
    const gamepad1 = makeGamepad(1);
    const gamepad2 = makeGamepad(2);
    const gamepads = [null, gamepad1, gamepad2];
    const selected = selectGamepads(gamepads);
    
    expect(selected.length).toBe(2);
    expect(selected[0]).toBe(gamepad1);
    expect(selected[1]).toBe(gamepad2);
});

it("selectGamepads returns only first two when more than two connected", () => {
    const gamepad0 = makeGamepad(0);
    const gamepad1 = makeGamepad(1);
    const gamepad2 = makeGamepad(2);
    const gamepad3 = makeGamepad(3);
    const gamepads = [gamepad0, gamepad1, gamepad2, gamepad3];
    const selected = selectGamepads(gamepads);
    
    expect(selected.length).toBe(2);
    expect(selected[0]).toBe(gamepad0);
    expect(selected[1]).toBe(gamepad1);
});

// Tests for getKeyboardControllerTarget
it("getKeyboardControllerTarget returns [1, 2] when no gamepads", () => {
    const targets = getKeyboardControllerTarget(0);
    expect(targets).toEqual([1, 2]);
});

it("getKeyboardControllerTarget returns [2] when one gamepad", () => {
    const targets = getKeyboardControllerTarget(1);
    expect(targets).toEqual([2]);
});

it("getKeyboardControllerTarget returns [] when two or more gamepads", () => {
    expect(getKeyboardControllerTarget(2)).toEqual([]);
    expect(getKeyboardControllerTarget(3)).toEqual([]);
});

it("getKeyboardControllerTarget routes keyboard to players 3 and 4 when Four Score is enabled and two gamepads are connected", () => {
    const targets = getKeyboardControllerTarget(2, true);
    expect(targets).toEqual([3, 4]);
});

it("getKeyboardControllerTarget routes keyboard to players 2 and 3 when Four Score is enabled and one gamepad is connected", () => {
    const targets = getKeyboardControllerTarget(1, true);
    expect(targets).toEqual([2, 3]);
});

it("getKeyboardControllerTarget follows Four Score unplug transition sequence", () => {
    expect(getKeyboardControllerTarget(2, true)).toEqual([3, 4]);
    expect(getKeyboardControllerTarget(1, true)).toEqual([2, 3]);
    expect(getKeyboardControllerTarget(0, true)).toEqual([1, 2]);
});

// Tests for shouldSuppressJoypadInput
it("shouldSuppressJoypadInput suppresses controller 1 when mouse-emulated controller on port 1", () => {
    const nes = makeNesStub({ arkanoidPort: 1 });
    expect(shouldSuppressJoypadInput(nes, 1)).toBe(true);
});

it("shouldSuppressJoypadInput suppresses controller 2 when mouse-emulated controller on port 2", () => {
    const nes = makeNesStub({ arkanoidPort: 2 });
    expect(shouldSuppressJoypadInput(nes, 2)).toBe(true);
});

it("shouldSuppressJoypadInput allows controller 1 when mouse-emulated controller on port 2", () => {
    const nes = makeNesStub({ arkanoidPort: 2 });
    expect(shouldSuppressJoypadInput(nes, 1)).toBe(false);
});

it("shouldSuppressJoypadInput allows controller 2 when mouse-emulated controller on port 1", () => {
    const nes = makeNesStub({ arkanoidPort: 1 });
    expect(shouldSuppressJoypadInput(nes, 2)).toBe(false);
});

it("shouldSuppressJoypadInput allows both controllers when no mouse-emulated controller", () => {
    const nes = makeNesStub({ arkanoidPort: null });
    expect(shouldSuppressJoypadInput(nes, 1)).toBe(false);
    expect(shouldSuppressJoypadInput(nes, 2)).toBe(false);
});
