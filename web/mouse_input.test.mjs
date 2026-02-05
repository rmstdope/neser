import test from "node:test";
import assert from "node:assert/strict";

import {
    applyJoypadButtonIfAllowed,
    applyMouseButton,
    applyMouseMotion,
    mapMouseXToScreenPosition
} from "./mouse_input.js";

function makeNesStub({ arkanoidPort = 0 } = {}) {
    const calls = {
        setButton: [],
        setMouseXPosition: [],
        setMouseLeftButton: []
    };

    const nes = {
        is_mouse_emulated_controller: (controller) => (arkanoidPort === controller),
        set_button: (controller, button, pressed) => {
            calls.setButton.push({ controller, button, pressed });
        },
        set_mouse_x_position: (position) => {
            calls.setMouseXPosition.push({ position });
        },
        set_mouse_left_button: (pressed) => {
            calls.setMouseLeftButton.push({ pressed });
        }
    };

    return { nes, calls };
}

test("mapMouseXToScreenPosition maps edges and center", () => {
    const width = 300;

    const left = mapMouseXToScreenPosition(0, width);
    const right = mapMouseXToScreenPosition(width - 1, width);
    const centerX = Math.floor((width - 1) / 2);
    const center = mapMouseXToScreenPosition(centerX, width);

    assert.equal(left, 0x62);
    assert.equal(right, 0xF2);
    assert.ok(center >= 165 && center <= 175);
});

test("mapMouseXToScreenPosition uses non-linear curve", () => {
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

test("applyMouseMotion updates position", () => {
    const { nes, calls } = makeNesStub({ arkanoidPort: 1 });
    const width = 320;
    const x = 240;

    const expected = mapMouseXToScreenPosition(x, width);
    applyMouseMotion(nes, x, width);

    assert.deepEqual(calls.setMouseXPosition, [{ position: expected }]);
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

