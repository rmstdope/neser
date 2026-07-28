import { expect, it } from "vitest";

import { loadRawButtonLayoutsFromDb, mapStandardGamepadState, selectPrimaryGamepad } from "./gamepad";

function makeButtons(pressedIndexes: number[] = []) {
  const buttons = Array.from({ length: 16 }, () => ({ pressed: false }));
  for (const index of pressedIndexes) {
    buttons[index] = { pressed: true };
  }
  return buttons;
}

it("maps standard gamepad buttons to NES inputs", () => {
  const gamepad = {
    buttons: makeButtons([0, 1, 8, 9, 12, 14]),
    axes: [0, 0, 0, 0]
  };

  const state = mapStandardGamepadState(gamepad as unknown as Gamepad);

  expect(state.a).toBe(true);
  expect(state.b).toBe(true);
  expect(state.select).toBe(true);
  expect(state.start).toBe(true);
  expect(state.up).toBe(true);
  expect(state.left).toBe(true);
  expect(state.down).toBe(false);
  expect(state.right).toBe(false);
});

it("maps standard gamepad shoulder buttons to GBA L and R inputs", () => {
  const gamepad = {
    buttons: makeButtons([4, 5]),
    axes: [0, 0, 0, 0]
  };

  const state = mapStandardGamepadState(gamepad as unknown as Gamepad);

  expect(state.l).toBe(true);
  expect(state.r).toBe(true);
  expect(state.a).toBe(false);
  expect(state.b).toBe(false);
});

it("maps standard gamepad west/north buttons to SNES Y/X inputs", () => {
  const gamepad = {
    buttons: makeButtons([2, 3]),
    axes: [0, 0, 0, 0]
  };

  const state = mapStandardGamepadState(gamepad as unknown as Gamepad);

  expect(state.y).toBe(true);
  expect(state.x).toBe(true);
  expect(state.a).toBe(false);
  expect(state.b).toBe(false);
});

it("maps left stick axes to NES directions", () => {
  const gamepad = {
    buttons: makeButtons(),
    axes: [-1, 1, 0, 0]
  };

  const state = mapStandardGamepadState(gamepad as unknown as Gamepad);

  expect(state.left).toBe(true);
  expect(state.down).toBe(true);
  expect(state.right).toBe(false);
  expect(state.up).toBe(false);
});

it("does not trigger directions at exact axis threshold", () => {
  const gamepad = {
    buttons: makeButtons(),
    axes: [0.5, -0.5, 0, 0]
  };

  const state = mapStandardGamepadState(gamepad as unknown as Gamepad);

  expect(state.right).toBe(false);
  expect(state.left).toBe(false);
  expect(state.up).toBe(false);
  expect(state.down).toBe(false);
});

it("prefers pressed if both d-pad and axes active", () => {
  const gamepad = {
    buttons: makeButtons([12, 13, 14, 15]),
    axes: [-1, -1, 0, 0]
  };

  const state = mapStandardGamepadState(gamepad as unknown as Gamepad);

  expect(state.up).toBe(true);
  expect(state.down).toBe(true);
  expect(state.left).toBe(true);
  expect(state.right).toBe(true);
});

const REPLICA_DB_LINE =
  "030000001f08000001e4000006010000,USB SNES gamepad,a:b2,b:b1,x:b3,y:b0,back:b8,start:b9,leftshoulder:b4,rightshoulder:b5,leftx:a0,lefty:a1,platform:Mac OS X,";
const MAC_UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)";

async function loadReplicaDb() {
  const fetchStub = (async () => ({ ok: true, text: async () => REPLICA_DB_LINE })) as unknown as typeof fetch;
  await loadRawButtonLayoutsFromDb(fetchStub, MAC_UA);
}

it("remaps the generic SNES USB replica pad when the browser exposes it unmapped", async () => {
  await loadReplicaDb();
  // Chrome id format. Raw HID button order on this pad: 0=X, 1=A, 2=B, 3=Y,
  // 8=Select, 9=Start — feeding it through the standard-layout reading made
  // physical X act as SNES B.
  const gamepad = {
    id: "USB gamepad (Vendor: 081f Product: e401)",
    mapping: "",
    buttons: makeButtons([2]),
    axes: [0, 0, 0, 0]
  };

  const state = mapStandardGamepadState(gamepad as unknown as Gamepad);

  expect(state.a).toBe(true); // physical B = south position
  expect(state.x).toBe(false);
  expect(state.y).toBe(false);
});

it("maps the replica pad's physical X to the north position, not south", async () => {
  await loadReplicaDb();
  const gamepad = {
    id: "081f-e401-USB gamepad", // Firefox id format
    mapping: "",
    buttons: makeButtons([0, 1, 3, 4, 5, 8, 9]),
    axes: [0, 0, 0, 0]
  };

  const state = mapStandardGamepadState(gamepad as unknown as Gamepad);

  expect(state.x).toBe(true); // physical X (raw 0) = north
  expect(state.b).toBe(true); // physical A (raw 1) = east
  expect(state.y).toBe(true); // physical Y (raw 3) = west
  expect(state.a).toBe(false); // south (physical B, raw 2) not pressed
  expect(state.select).toBe(true);
  expect(state.start).toBe(true);
  expect(state.l).toBe(true); // verified on hardware: L is raw b4
  expect(state.r).toBe(true); // verified on hardware: R is raw b5
});

it("does not remap the replica pad when the browser already standard-maps it", () => {
  const gamepad = {
    id: "USB gamepad (STANDARD GAMEPAD Vendor: 081f Product: e401)",
    mapping: "standard",
    buttons: makeButtons([0]),
    axes: [0, 0, 0, 0]
  };

  const state = mapStandardGamepadState(gamepad as unknown as Gamepad);

  expect(state.a).toBe(true); // standard b0 = south
  expect(state.x).toBe(false);
});

it("uses layouts loaded from gamecontrollerdb.txt for other unmapped pads", async () => {
  // Fake db entry for vendor 045e / product 028e with a scrambled layout.
  const dbLine =
    "030000005e0400008e02000000000000,Fake pad,a:b3,b:b2,x:b1,y:b0,back:b6,start:b7,platform:Mac OS X,";
  const fetchStub = (async () => ({
    ok: true,
    text: async () => dbLine
  })) as unknown as typeof fetch;

  await loadRawButtonLayoutsFromDb(fetchStub, "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)");

  const gamepad = {
    id: "Fake pad (Vendor: 045e Product: 028e)",
    mapping: "",
    buttons: makeButtons([3, 6]),
    axes: [0, 0, 0, 0]
  };

  const state = mapStandardGamepadState(gamepad as unknown as Gamepad);

  expect(state.a).toBe(true); // db says south is raw b3
  expect(state.select).toBe(true); // back -> raw b6
  expect(state.x).toBe(false);
});

it("falls back to the standard layout for unmapped pads absent from the db", () => {
  const gamepad = {
    id: "Mystery pad (Vendor: 1234 Product: abcd)",
    mapping: "",
    buttons: makeButtons([0]),
    axes: [0, 0, 0, 0]
  };

  const state = mapStandardGamepadState(gamepad as unknown as Gamepad);

  expect(state.a).toBe(true); // standard b0 = south
});

it("selects first connected gamepad", () => {
  const gamepads = [
    null,
    { connected: true, buttons: makeButtons(), axes: [0, 0, 0, 0] },
    { connected: true, buttons: makeButtons(), axes: [0, 0, 0, 0] }
  ];

  const selected = selectPrimaryGamepad(gamepads as unknown as (Gamepad | null)[]);

  expect(selected).toBe(gamepads[1]);
});

it("returns null when no gamepad connected", () => {
  const gamepads = [null, { connected: false }, undefined];

  const selected = selectPrimaryGamepad(gamepads as unknown as (Gamepad | null)[]);

  expect(selected).toBe(null);
});
