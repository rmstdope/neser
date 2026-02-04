import test from "node:test";
import assert from "node:assert/strict";

import {
    applyJoypadButtonIfAllowed,
    applyMouseButton,
    applyMouseMotion,
    mapMouseXToScreenPosition
} from "./mouse_input.js";

function makeNesStub({ paddleEnabled = false } = {}) {
    const calls = {
        setButton: [],
        setPaddlePosition: [],
        setPaddleTrigger: []
    };

    const nes = {
        paddle1_enabled: () => paddleEnabled,
        paddle_port: () => paddleEnabled ? 1 : null,
        set_button: (controller, button, pressed) => {
            calls.setButton.push({ controller, button, pressed });
        },
        set_mouse_x_position: (port, position) => {
            calls.setPaddlePosition.push({ port, position });
        },
        set_mouse_left_button: (port, pressed) => {
            calls.setPaddleTrigger.push({ port, pressed });
        }
    };

    return { nes, calls, setPaddleEnabled: (enabled) => { paddleEnabled = enabled; } };
}

test("mapMouseXToPaddlePosition maps edges and center", () => {
    const width = 300;

    const left = mapMouseXToScreenPosition(0, width);
    const right = mapMouseXToScreenPosition(width - 1, width);
    const centerX = Math.floor((width - 1) / 2);
    const center = mapMouseXToScreenPosition(centerX, width);

    assert.equal(left, 0x62);
    assert.equal(right, 0xF2);
    assert.ok(center >= 165 && center <= 175);
});

test("mapMouseXToPaddlePosition uses non-linear curve", () => {
    const width = 400;
    const centerA = 200;
    const centerB = 220;
    const edgeA = 360;
    const edgeB = 380;

    const centerDelta =
        mapMouseXToScreenPosition(centerB, width) -
        mapMouseXToScreenPosition(centerA, width);
    const edgeDelta =
        mapMouseXToScreenPosition(edgeB, width) -
        mapMouseXToScreenPosition(edgeA, width);

    assert.ok(edgeDelta > centerDelta);
});

test("applyPaddleMouseMotion updates position when enabled", () => {
    const { nes, calls } = makeNesStub({ paddleEnabled: true });
    const width = 320;
    const x = 240;

    const expected = mapMouseXToScreenPosition(x, width);
    applyMouseMotion(nes, x, width);

    assert.deepEqual(calls.setPaddlePosition, [{ port: 1, position: expected }]);
});

test("applyPaddleMouseMotion ignored when disabled", () => {
    const { nes, calls } = makeNesStub({ paddleEnabled: false });
    applyMouseMotion(nes, 240, 320);

    assert.deepEqual(calls.setPaddlePosition, []);
});

test("applyPaddleMouseButton maps left button to trigger", () => {
    const { nes, calls } = makeNesStub({ paddleEnabled: true });

    applyMouseButton(nes, 0, true);
    applyMouseButton(nes, 0, false);

    assert.deepEqual(calls.setPaddleTrigger, [{ port: 1, pressed: true }, { port: 1, pressed: false }]);
});

test("applyPaddleMouseButton ignores non-left buttons", () => {
    const { nes, calls } = makeNesStub({ paddleEnabled: true });

    applyMouseButton(nes, 1, true);

    assert.deepEqual(calls.setPaddleTrigger, []);
});

test("applyPaddleMouseButton ignored when disabled", () => {
    const { nes, calls } = makeNesStub({ paddleEnabled: false });

    applyMouseButton(nes, 0, true);

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

// Tests for dynamic paddle port detection
function makeNesStubWithPort({ paddlePort = null } = {}) {
    const calls = {
        setButton: [],
        setPaddlePosition: [],
        setPaddleTrigger: []
    };

    const nes = {
        paddle_port: () => paddlePort,
        set_button: (controller, button, pressed) => {
            calls.setButton.push({ controller, button, pressed });
        },
        set_mouse_x_position: (port, position) => {
            calls.setPaddlePosition.push({ port, position });
        },
        set_mouse_left_button: (port, pressed) => {
            calls.setPaddleTrigger.push({ port, pressed });
        }
    };

    return { nes, calls };
}

test("applyJoypadButtonIfAllowed suppresses controller 2 when paddle on port 2", () => {
    const { nes, calls } = makeNesStubWithPort({ paddlePort: 2 });

    applyJoypadButtonIfAllowed(nes, 2, 0, true);

    assert.deepEqual(calls.setButton, []);
});

test("applyJoypadButtonIfAllowed allows controller 1 when paddle on port 2", () => {
    const { nes, calls } = makeNesStubWithPort({ paddlePort: 2 });

    applyJoypadButtonIfAllowed(nes, 1, 0, true);

    assert.deepEqual(calls.setButton, [
        { controller: 1, button: 0, pressed: true }
    ]);
});

test("applyPaddleMouseMotion works with paddle on port 2", () => {
    const { nes, calls } = makeNesStubWithPort({ paddlePort: 2 });
    const width = 320;
    const x = 240;

    const expected = mapMouseXToScreenPosition(x, width);
    applyMouseMotion(nes, x, width);

    assert.deepEqual(calls.setPaddlePosition, [{ port: 2, position: expected }]);
});

test("applyPaddleMouseButton works with paddle on port 2", () => {
    const { nes, calls } = makeNesStubWithPort({ paddlePort: 2 });

    applyMouseButton(nes, 0, true);

    assert.deepEqual(calls.setPaddleTrigger, [{ port: 2, pressed: true }]);
});
