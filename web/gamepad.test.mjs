import test from "node:test";
import assert from "node:assert/strict";

import { mapStandardGamepadState, selectPrimaryGamepad } from "./gamepad.js";

function makeButtons(pressedIndexes = []) {
  const buttons = Array.from({ length: 16 }, () => ({ pressed: false }));
  for (const index of pressedIndexes) {
    buttons[index] = { pressed: true };
  }
  return buttons;
}

test("maps standard gamepad buttons to NES inputs", () => {
  const gamepad = {
    buttons: makeButtons([0, 1, 8, 9, 12, 14]),
    axes: [0, 0, 0, 0]
  };

  const state = mapStandardGamepadState(gamepad);

  assert.equal(state.a, true);
  assert.equal(state.b, true);
  assert.equal(state.select, true);
  assert.equal(state.start, true);
  assert.equal(state.up, true);
  assert.equal(state.left, true);
  assert.equal(state.down, false);
  assert.equal(state.right, false);
});

test("maps left stick axes to NES directions", () => {
  const gamepad = {
    buttons: makeButtons(),
    axes: [-1, 1, 0, 0]
  };

  const state = mapStandardGamepadState(gamepad);

  assert.equal(state.left, true);
  assert.equal(state.down, true);
  assert.equal(state.right, false);
  assert.equal(state.up, false);
});

test("selects first connected gamepad", () => {
  const gamepads = [
    null,
    { connected: true, buttons: makeButtons(), axes: [0, 0, 0, 0] },
    { connected: true, buttons: makeButtons(), axes: [0, 0, 0, 0] }
  ];

  const selected = selectPrimaryGamepad(gamepads);

  assert.equal(selected, gamepads[1]);
});

test("returns null when no gamepad connected", () => {
  const gamepads = [null, { connected: false }, undefined];

  const selected = selectPrimaryGamepad(gamepads);

  assert.equal(selected, null);
});
