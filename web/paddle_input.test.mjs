import test from "node:test";
import assert from "node:assert/strict";

import {
    applyJoypadButtonIfAllowed,
    applyPaddleMouseButton,
    applyPaddleMouseMotion,
    mapMouseXToPaddlePosition
} from "./paddle_input.js";

function makeNesStub({ paddleEnabled = false } = {}) {
    const calls = {
        setButton: [],
        setPaddlePosition: [],
        setPaddleTrigger: []
    };

    const nes = {
        paddle1_enabled: () => paddleEnabled,
        set_button: (controller, button, pressed) => {
            calls.setButton.push({ controller, button, pressed });
        },
        set_paddle1_position: (position) => {
            calls.setPaddlePosition.push(position);
        },
        set_paddle1_trigger: (pressed) => {
            calls.setPaddleTrigger.push(pressed);
        }
    };

    return { nes, calls, setPaddleEnabled: (enabled) => { paddleEnabled = enabled; } };
}

test("mapMouseXToPaddlePosition maps edges and center", () => {
    const width = 300;

    const left = mapMouseXToPaddlePosition(0, width);
    const right = mapMouseXToPaddlePosition(width - 1, width);
    const centerX = Math.floor((width - 1) / 2);
    const center = mapMouseXToPaddlePosition(centerX, width);

    assert.equal(left, 0);
    assert.equal(right, 255);
    assert.ok(center >= 120 && center <= 135);
});

test("mapMouseXToPaddlePosition uses non-linear curve", () => {
    const width = 400;
    const centerA = 200;
    const centerB = 220;
    const edgeA = 360;
    const edgeB = 380;

    const centerDelta =
        mapMouseXToPaddlePosition(centerB, width) -
        mapMouseXToPaddlePosition(centerA, width);
    const edgeDelta =
        mapMouseXToPaddlePosition(edgeB, width) -
        mapMouseXToPaddlePosition(edgeA, width);

    assert.ok(edgeDelta > centerDelta);
});

test("applyPaddleMouseMotion updates position when enabled", () => {
    const { nes, calls } = makeNesStub({ paddleEnabled: true });
    const width = 320;
    const x = 240;

    const expected = mapMouseXToPaddlePosition(x, width);
    applyPaddleMouseMotion(nes, x, width);

    assert.deepEqual(calls.setPaddlePosition, [expected]);
});

test("applyPaddleMouseMotion ignored when disabled", () => {
    const { nes, calls } = makeNesStub({ paddleEnabled: false });
    applyPaddleMouseMotion(nes, 240, 320);

    assert.deepEqual(calls.setPaddlePosition, []);
});

test("applyPaddleMouseButton maps left button to trigger", () => {
    const { nes, calls } = makeNesStub({ paddleEnabled: true });

    applyPaddleMouseButton(nes, 0, true);
    applyPaddleMouseButton(nes, 0, false);

    assert.deepEqual(calls.setPaddleTrigger, [true, false]);
});

test("applyPaddleMouseButton ignores non-left buttons", () => {
    const { nes, calls } = makeNesStub({ paddleEnabled: true });

    applyPaddleMouseButton(nes, 1, true);

    assert.deepEqual(calls.setPaddleTrigger, []);
});

test("applyPaddleMouseButton ignored when disabled", () => {
    const { nes, calls } = makeNesStub({ paddleEnabled: false });

    applyPaddleMouseButton(nes, 0, true);

    assert.deepEqual(calls.setPaddleTrigger, []);
});

test("applyJoypadButtonIfAllowed suppresses controller 1 in paddle mode", () => {
    const { nes, calls } = makeNesStub({ paddleEnabled: true });

    applyJoypadButtonIfAllowed(nes, 1, 0, true);

    assert.deepEqual(calls.setButton, []);
});

test("applyJoypadButtonIfAllowed allows controller 2 in paddle mode", () => {
    const { nes, calls } = makeNesStub({ paddleEnabled: true });

    applyJoypadButtonIfAllowed(nes, 2, 1, true);

    assert.deepEqual(calls.setButton, [
        { controller: 2, button: 1, pressed: true }
    ]);
});

test("applyJoypadButtonIfAllowed allows controller 1 when paddle disabled", () => {
    const { nes, calls } = makeNesStub({ paddleEnabled: false });

    applyJoypadButtonIfAllowed(nes, 1, 7, false);

    assert.deepEqual(calls.setButton, [
        { controller: 1, button: 7, pressed: false }
    ]);
});
