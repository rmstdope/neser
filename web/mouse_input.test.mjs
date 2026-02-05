import test from "node:test";
import assert from "node:assert/strict";

import {
    applyJoypadButtonIfAllowed,
    applyMouseButton,
    applyMouseMotion,
    mapMouseXToPaddlePosition,
    mapMouseXToZapperPosition,
    mapMouseYToZapperPosition,
    isZapperActive
} from "./mouse_input.js";

function makeNesStub({ arkanoidPort = 0, zapperPort = 0 } = {}) {
    const calls = {
        setButton: [],
        setMouseXPosition: [],
        setMouseYPosition: [],
        setMouseLeftButton: []
    };

    const nes = {
        is_mouse_emulated_controller: (controller) => (arkanoidPort === controller),
        is_zapper_active: (port) => (zapperPort === port),
        set_button: (controller, button, pressed) => {
            calls.setButton.push({ controller, button, pressed });
        },
        set_mouse_x_position: (position) => {
            calls.setMouseXPosition.push({ position });
        },
        set_mouse_y_position: (position) => {
            calls.setMouseYPosition.push({ position });
        },
        set_mouse_left_button: (pressed) => {
            calls.setMouseLeftButton.push({ pressed });
        }
    };

    return { nes, calls };
}

test("mapMouseXToPaddlePosition maps edges and center", () => {
    const width = 300;

    const left = mapMouseXToPaddlePosition(0, width);
    const right = mapMouseXToPaddlePosition(width - 1, width);
    const centerX = Math.floor((width - 1) / 2);
    const center = mapMouseXToPaddlePosition(centerX, width);

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
        mapMouseXToPaddlePosition(centerB, width) -
        mapMouseXToPaddlePosition(centerA, width);
    const edgeDelta =
        mapMouseXToPaddlePosition(edgeB, width) -
        mapMouseXToPaddlePosition(edgeA, width);

    assert.ok(edgeDelta > centerDelta);
});

test("mapMouseXToZapperPosition maps edges and center linearly", () => {
    const width = 320;

    const left = mapMouseXToZapperPosition(0, width);
    const right = mapMouseXToZapperPosition(width - 1, width);
    const centerX = Math.floor((width - 1) / 2);
    const center = mapMouseXToZapperPosition(centerX, width);

    assert.equal(left, 0);
    assert.equal(right, 255);
    assert.ok(center >= 126 && center <= 128);
});

test("mapMouseYToZapperPosition maps top and bottom edges", () => {
    const height = 480;

    const top = mapMouseYToZapperPosition(0, height);
    const bottom = mapMouseYToZapperPosition(height - 1, height);

    assert.equal(top, 0);
    assert.equal(bottom, 239);
});

test("mapMouseYToZapperPosition maps center", () => {
    const height = 480;
    const centerY = Math.floor((height - 1) / 2);
    const center = mapMouseYToZapperPosition(centerY, height);

    assert.ok(center >= 119 && center <= 120);
});

test("mapMouseYToZapperPosition clamps to bounds", () => {
    const height = 480;

    const negative = mapMouseYToZapperPosition(-10, height);
    const tooLarge = mapMouseYToZapperPosition(1000, height);

    assert.equal(negative, 0);
    assert.equal(tooLarge, 239);
});

test("applyMouseMotion updates Arkanoid position", () => {
    const { nes, calls } = makeNesStub({ arkanoidPort: 1 });
    const width = 320;
    const height = 480;
    const x = 240;
    const y = 360;

    const expectedX = mapMouseXToPaddlePosition(x, width);
    applyMouseMotion(nes, x, y, width, height);

    assert.deepEqual(calls.setMouseXPosition, [{ position: expectedX }]);
    // Arkanoid doesn't set Y position
    assert.deepEqual(calls.setMouseYPosition, []);
});

test("applyMouseMotion updates Zapper position", () => {
    const { nes, calls } = makeNesStub({ zapperPort: 1 });
    const width = 320;
    const height = 480;
    const x = 240;
    const y = 360;

    const expectedX = mapMouseXToZapperPosition(x, width);
    const expectedY = mapMouseYToZapperPosition(y, height);
    applyMouseMotion(nes, x, y, width, height);

    assert.deepEqual(calls.setMouseXPosition, [{ position: expectedX }]);
    assert.deepEqual(calls.setMouseYPosition, [{ position: expectedY }]);
});

test("applyMouseButton maps left button to trigger", () => {
    const { nes, calls } = makeNesStub({ arkanoidPort: 1 });

    applyMouseButton(nes, 0, true);
    applyMouseButton(nes, 0, false);

    assert.deepEqual(calls.setMouseLeftButton, [{ pressed: true }, { pressed: false }]);
});

test("applyJoypadButtonIfAllowed suppresses controller 1 in mouse mode", () => {
    const { nes, calls } = makeNesStub({ arkanoidPort: 1 });

    applyJoypadButtonIfAllowed(nes, 1, 0, true);

    assert.deepEqual(calls.setButton, []);
});

test("applyJoypadButtonIfAllowed allows controller 2 in mouse mode", () => {
    const { nes, calls } = makeNesStub({ arkanoidPort: 1 });

    applyJoypadButtonIfAllowed(nes, 2, 1, true);

    assert.deepEqual(calls.setButton, [
        { controller: 2, button: 1, pressed: true }
    ]);
});

test("applyJoypadButtonIfAllowed allows controller 1 when mouse disabled", () => {
    const { nes, calls } = makeNesStub({ });

    applyJoypadButtonIfAllowed(nes, 1, 7, false);

    assert.deepEqual(calls.setButton, [
        { controller: 1, button: 7, pressed: false }
    ]);
});


test("applyJoypadButtonIfAllowed suppresses controller 2 when mouse on port 2", () => {
    const { nes, calls } = makeNesStub({ arkanoidPort: 2 });

    applyJoypadButtonIfAllowed(nes, 2, 0, true);

    assert.deepEqual(calls.setButton, []);
});

test("applyJoypadButtonIfAllowed allows controller 1 when mouse on port 2", () => {
    const { nes, calls } = makeNesStub({ arkanoidPort: 2 });

    applyJoypadButtonIfAllowed(nes, 1, 0, true);

    assert.deepEqual(calls.setButton, [
        { controller: 1, button: 0, pressed: true }
    ]);
});

test("isZapperActive returns true when Zapper on port 1", () => {
    const { nes } = makeNesStub({ zapperPort: 1 });
    assert.equal(isZapperActive(nes), true);
});

test("isZapperActive returns true when Zapper on port 2", () => {
    const { nes } = makeNesStub({ zapperPort: 2 });
    assert.equal(isZapperActive(nes), true);
});

test("isZapperActive returns false when no Zapper", () => {
    const { nes } = makeNesStub({ zapperPort: 0 });
    assert.equal(isZapperActive(nes), false);
});
