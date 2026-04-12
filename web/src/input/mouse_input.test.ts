import { expect, it } from "vitest";

import {
    applyJoypadButtonIfAllowed,
    applyMouseButton,
    applyMouseMotion,
    mapMouseXToPaddlePosition,
    mapMouseAxisToSnesMousePosition,
    mapMouseXToZapperPosition,
    mapMouseYToZapperPosition,
    isZapperActive
} from "./mouse_input";

function makeNesStub({ arkanoidPort = 0, zapperPort = 0, snesMousePorts = [] }: { arkanoidPort?: number; zapperPort?: number; snesMousePorts?: number[] } = {}) {
    const calls = {
        setButton: [] as { controller: number; button: number; pressed: boolean }[],
        setMouseXPosition: [] as { position: number }[],
        setMouseYPosition: [] as { position: number }[],
        setMouseLeftButton: [] as { pressed: boolean }[],
        setMouseRightButton: [] as { pressed: boolean }[]
    };

    const nes = {
        is_mouse_emulated_controller: (controller: number) => (arkanoidPort === controller),
        is_zapper_active: (port: number) => (zapperPort === port),
        is_snes_mouse_active: (port: number) => snesMousePorts.includes(port),
        set_button: (controller: number, button: number, pressed: boolean) => {
            calls.setButton.push({ controller, button, pressed });
        },
        set_mouse_x_position: (position: number) => {
            calls.setMouseXPosition.push({ position });
        },
        set_mouse_y_position: (position: number) => {
            calls.setMouseYPosition.push({ position });
        },
        set_mouse_left_button: (pressed: boolean) => {
            calls.setMouseLeftButton.push({ pressed });
        },
        set_mouse_right_button: (pressed: boolean) => {
            calls.setMouseRightButton.push({ pressed });
        }
    };

    return { nes, calls };
}

it("mapMouseXToPaddlePosition maps edges and center", () => {
    const width = 300;

    const left = mapMouseXToPaddlePosition(0, width);
    const right = mapMouseXToPaddlePosition(width - 1, width);
    const centerX = Math.floor((width - 1) / 2);
    const center = mapMouseXToPaddlePosition(centerX, width);

    expect(left).toBe(0x62);
    expect(right).toBe(0xF2);
    expect(center >= 165 && center <= 175).toBeTruthy();
});

it("mapMouseXToPaddlePosition uses non-linear curve", () => {
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

    expect(edgeDelta > centerDelta).toBeTruthy();
});

it("mapMouseXToZapperPosition maps edges and center linearly", () => {
    const width = 320;

    const left = mapMouseXToZapperPosition(0, width);
    const right = mapMouseXToZapperPosition(width - 1, width);
    const centerX = Math.floor((width - 1) / 2);
    const center = mapMouseXToZapperPosition(centerX, width);

    expect(left).toBe(0);
    expect(right).toBe(255);
    expect(center >= 126 && center <= 128).toBeTruthy();
});

it("mapMouseYToZapperPosition maps top and bottom edges", () => {
    const height = 480;

    const top = mapMouseYToZapperPosition(0, height);
    const bottom = mapMouseYToZapperPosition(height - 1, height);

    expect(top).toBe(0);
    expect(bottom).toBe(239);
});

it("mapMouseYToZapperPosition maps center", () => {
    const height = 480;
    const centerY = Math.floor((height - 1) / 2);
    const center = mapMouseYToZapperPosition(centerY, height);

    expect(center >= 119 && center <= 120).toBeTruthy();
});

it("mapMouseYToZapperPosition clamps to bounds", () => {
    const height = 480;

    const negative = mapMouseYToZapperPosition(-10, height);
    const tooLarge = mapMouseYToZapperPosition(1000, height);

    expect(negative).toBe(0);
    expect(tooLarge).toBe(239);
});

it("applyMouseMotion updates Arkanoid position", () => {
    const { nes, calls } = makeNesStub({ arkanoidPort: 1 });
    const width = 320;
    const height = 480;
    const x = 240;
    const y = 360;

    const expectedX = mapMouseXToPaddlePosition(x, width);
    applyMouseMotion(nes as any, x, y, width, height);

    expect(calls.setMouseXPosition).toEqual([{ position: expectedX }]);
    // Arkanoid doesn't set Y position
    expect(calls.setMouseYPosition).toEqual([]);
});

it("applyMouseMotion updates Zapper position", () => {
    const { nes, calls } = makeNesStub({ zapperPort: 1 });
    const width = 320;
    const height = 480;
    const x = 240;
    const y = 360;

    const expectedX = mapMouseXToZapperPosition(x, width);
    const expectedY = mapMouseYToZapperPosition(y, height);
    applyMouseMotion(nes as any, x, y, width, height);

    expect(calls.setMouseXPosition).toEqual([{ position: expectedX }]);
    expect(calls.setMouseYPosition).toEqual([{ position: expectedY }]);
});

it("applyMouseButton maps left button to trigger", () => {
    const { nes, calls } = makeNesStub({ arkanoidPort: 1 });

    applyMouseButton(nes as any, 0, true);
    applyMouseButton(nes as any, 0, false);

    expect(calls.setMouseLeftButton).toEqual([{ pressed: true }, { pressed: false }]);
});

it("applyMouseButton maps right button to SNES mouse secondary button", () => {
    const { nes, calls } = makeNesStub({ snesMousePorts: [1] });

    applyMouseButton(nes as any, 2, true);
    applyMouseButton(nes as any, 2, false);

    expect(calls.setMouseRightButton).toEqual([{ pressed: true }, { pressed: false }]);
});

it("applyMouseMotion updates SNES mouse position on both axes", () => {
    const { nes, calls } = makeNesStub({ snesMousePorts: [1] });
    const width = 320;
    const height = 480;
    const x = 240;
    const y = 360;

    const expectedX = mapMouseAxisToSnesMousePosition(x, width);
    const expectedY = mapMouseAxisToSnesMousePosition(y, height);
    applyMouseMotion(nes as any, x, y, width, height);

    expect(calls.setMouseXPosition).toEqual([{ position: expectedX }]);
    expect(calls.setMouseYPosition).toEqual([{ position: expectedY }]);
});

it("applyJoypadButtonIfAllowed suppresses controller 1 in mouse mode", () => {
    const { nes, calls } = makeNesStub({ arkanoidPort: 1 });

    applyJoypadButtonIfAllowed(nes as any, 1, 0, true);

    expect(calls.setButton).toEqual([]);
});

it("applyJoypadButtonIfAllowed allows controller 2 in mouse mode", () => {
    const { nes, calls } = makeNesStub({ arkanoidPort: 1 });

    applyJoypadButtonIfAllowed(nes as any, 2, 1, true);

    expect(calls.setButton).toEqual([
        { controller: 2, button: 1, pressed: true }
    ]);
});

it("applyJoypadButtonIfAllowed allows controller 1 when mouse disabled", () => {
    const { nes, calls } = makeNesStub({ });

    applyJoypadButtonIfAllowed(nes as any, 1, 7, false);

    expect(calls.setButton).toEqual([
        { controller: 1, button: 7, pressed: false }
    ]);
});


it("applyJoypadButtonIfAllowed suppresses controller 2 when mouse on port 2", () => {
    const { nes, calls } = makeNesStub({ arkanoidPort: 2 });

    applyJoypadButtonIfAllowed(nes as any, 2, 0, true);

    expect(calls.setButton).toEqual([]);
});

it("applyJoypadButtonIfAllowed allows controller 1 when mouse on port 2", () => {
    const { nes, calls } = makeNesStub({ arkanoidPort: 2 });

    applyJoypadButtonIfAllowed(nes as any, 1, 0, true);

    expect(calls.setButton).toEqual([
        { controller: 1, button: 0, pressed: true }
    ]);
});

it("isZapperActive returns true when Zapper on port 1", () => {
    const { nes } = makeNesStub({ zapperPort: 1 });
    expect(isZapperActive(nes as any)).toBe(true);
});

it("isZapperActive returns true when Zapper on port 2", () => {
    const { nes } = makeNesStub({ zapperPort: 2 });
    expect(isZapperActive(nes as any)).toBe(true);
});

it("isZapperActive returns false when no Zapper", () => {
    const { nes } = makeNesStub({ zapperPort: 0 });
    expect(isZapperActive(nes as any)).toBe(false);
});
