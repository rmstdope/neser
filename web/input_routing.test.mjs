import test from "node:test";
import assert from "node:assert/strict";

import { 
    selectGamepads,
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

function makeNesStub({ paddlePort = null } = {}) {
    return {
        paddle_port: () => paddlePort
    };
}

// Tests for selectGamepads
test("selectGamepads returns empty array when no gamepads connected", () => {
    const gamepads = [null, { connected: false }, undefined];
    const selected = selectGamepads(gamepads);
    assert.deepEqual(selected, []);
});

test("selectGamepads returns one gamepad when only one connected", () => {
    const gamepad1 = makeGamepad(1);
    const gamepads = [null, gamepad1, null];
    const selected = selectGamepads(gamepads);
    
    assert.equal(selected.length, 1);
    assert.equal(selected[0], gamepad1);
});

test("selectGamepads returns two gamepads when two connected", () => {
    const gamepad1 = makeGamepad(1);
    const gamepad2 = makeGamepad(2);
    const gamepads = [null, gamepad1, gamepad2];
    const selected = selectGamepads(gamepads);
    
    assert.equal(selected.length, 2);
    assert.equal(selected[0], gamepad1);
    assert.equal(selected[1], gamepad2);
});

test("selectGamepads returns only first two when more than two connected", () => {
    const gamepad0 = makeGamepad(0);
    const gamepad1 = makeGamepad(1);
    const gamepad2 = makeGamepad(2);
    const gamepad3 = makeGamepad(3);
    const gamepads = [gamepad0, gamepad1, gamepad2, gamepad3];
    const selected = selectGamepads(gamepads);
    
    assert.equal(selected.length, 2);
    assert.equal(selected[0], gamepad0);
    assert.equal(selected[1], gamepad1);
});

// Tests for getKeyboardControllerTarget
test("getKeyboardControllerTarget returns 1 when no gamepads", () => {
    const target = getKeyboardControllerTarget(0);
    assert.equal(target, 1);
});

test("getKeyboardControllerTarget returns 2 when one gamepad", () => {
    const target = getKeyboardControllerTarget(1);
    assert.equal(target, 2);
});

test("getKeyboardControllerTarget returns null when two or more gamepads", () => {
    assert.equal(getKeyboardControllerTarget(2), null);
    assert.equal(getKeyboardControllerTarget(3), null);
});

// Tests for shouldSuppressJoypadInput
test("shouldSuppressJoypadInput suppresses controller 1 when paddle on port 1", () => {
    const nes = makeNesStub({ paddlePort: 1 });
    assert.equal(shouldSuppressJoypadInput(nes, 1), true);
});

test("shouldSuppressJoypadInput suppresses controller 2 when paddle on port 2", () => {
    const nes = makeNesStub({ paddlePort: 2 });
    assert.equal(shouldSuppressJoypadInput(nes, 2), true);
});

test("shouldSuppressJoypadInput allows controller 1 when paddle on port 2", () => {
    const nes = makeNesStub({ paddlePort: 2 });
    assert.equal(shouldSuppressJoypadInput(nes, 1), false);
});

test("shouldSuppressJoypadInput allows controller 2 when paddle on port 1", () => {
    const nes = makeNesStub({ paddlePort: 1 });
    assert.equal(shouldSuppressJoypadInput(nes, 2), false);
});

test("shouldSuppressJoypadInput allows both controllers when no paddle", () => {
    const nes = makeNesStub({ paddlePort: null });
    assert.equal(shouldSuppressJoypadInput(nes, 1), false);
    assert.equal(shouldSuppressJoypadInput(nes, 2), false);
});
