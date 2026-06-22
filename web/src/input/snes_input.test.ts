import { describe, expect, it } from "vitest";

import {
    isSnesMouseActive,
    isSnesSuperScopeActive,
    applySnesMouseDelta,
    applySnesMouseButton,
    mapSnesScreenX,
    mapSnesScreenY,
    applySnesSuperScopePosition,
    applySnesSuperScopeButton,
    shouldSuppressSnesJoypadInput,
} from "./snes_input";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeSnesStub({
    mousePorts = [] as number[],
    superScopePorts = [] as number[],
    multitapPorts = [] as number[],
} = {}) {
    const calls = {
        addMouseDelta: [] as { port: number; dx: number; dy: number }[],
        setMouseLeftButton: [] as { port: number; pressed: boolean }[],
        setMouseRightButton: [] as { port: number; pressed: boolean }[],
        setSuperScopePosition: [] as { port: number; x: number; y: number }[],
        setSuperScopeTrigger: [] as { port: number; pressed: boolean }[],
        setSuperScopeCursor: [] as { port: number; pressed: boolean }[],
        setSuperScopeTurbo: [] as { port: number; pressed: boolean }[],
        setSuperScopePause: [] as { port: number; pressed: boolean }[],
    };

    const snes = {
        has_mouse: () => mousePorts.length > 0,
        has_mouse_on_port: (port: number) => mousePorts.includes(port),
        has_superscope: () => superScopePorts.length > 0,
        has_superscope_on_port: (port: number) => superScopePorts.includes(port),
        is_multitap_on_port: (port: number) => multitapPorts.includes(port),
        add_mouse_delta: (port: number, dx: number, dy: number) => {
            calls.addMouseDelta.push({ port, dx, dy });
        },
        set_mouse_left_button: (port: number, pressed: boolean) => {
            calls.setMouseLeftButton.push({ port, pressed });
        },
        set_mouse_right_button: (port: number, pressed: boolean) => {
            calls.setMouseRightButton.push({ port, pressed });
        },
        set_superscope_position: (port: number, x: number, y: number) => {
            calls.setSuperScopePosition.push({ port, x, y });
        },
        set_superscope_trigger: (port: number, pressed: boolean) => {
            calls.setSuperScopeTrigger.push({ port, pressed });
        },
        set_superscope_cursor: (port: number, pressed: boolean) => {
            calls.setSuperScopeCursor.push({ port, pressed });
        },
        set_superscope_turbo: (port: number, pressed: boolean) => {
            calls.setSuperScopeTurbo.push({ port, pressed });
        },
        set_superscope_pause: (port: number, pressed: boolean) => {
            calls.setSuperScopePause.push({ port, pressed });
        },
    };

    return { snes, calls };
}

// ---------------------------------------------------------------------------
// isSnesMouseActive
// ---------------------------------------------------------------------------

it("isSnesMouseActive returns false when no mouse configured", () => {
    const { snes } = makeSnesStub();
    expect(isSnesMouseActive(snes)).toBe(false);
});

it("isSnesMouseActive returns true when mouse on port 1", () => {
    const { snes } = makeSnesStub({ mousePorts: [1] });
    expect(isSnesMouseActive(snes)).toBe(true);
});

it("isSnesMouseActive returns true when mouse on port 2", () => {
    const { snes } = makeSnesStub({ mousePorts: [2] });
    expect(isSnesMouseActive(snes)).toBe(true);
});

// ---------------------------------------------------------------------------
// isSnesSuperScopeActive
// ---------------------------------------------------------------------------

it("isSnesSuperScopeActive returns false when no superscope configured", () => {
    const { snes } = makeSnesStub();
    expect(isSnesSuperScopeActive(snes)).toBe(false);
});

it("isSnesSuperScopeActive returns true when superscope on port 2", () => {
    const { snes } = makeSnesStub({ superScopePorts: [2] });
    expect(isSnesSuperScopeActive(snes)).toBe(true);
});

// ---------------------------------------------------------------------------
// applySnesMouseDelta
// ---------------------------------------------------------------------------

it("applySnesMouseDelta sends delta to port 1", () => {
    const { snes, calls } = makeSnesStub({ mousePorts: [1] });
    applySnesMouseDelta(snes, 1, 5, -3);
    expect(calls.addMouseDelta).toEqual([{ port: 1, dx: 5, dy: -3 }]);
});

it("applySnesMouseDelta sends delta to port 2", () => {
    const { snes, calls } = makeSnesStub({ mousePorts: [2] });
    applySnesMouseDelta(snes, 2, 10, 20);
    expect(calls.addMouseDelta).toEqual([{ port: 2, dx: 10, dy: 20 }]);
});

it("applySnesMouseDelta does nothing when called with default port (0)", () => {
    const { snes, calls } = makeSnesStub({ mousePorts: [2] });
    applySnesMouseDelta(snes);
    expect(calls.addMouseDelta).toHaveLength(0);
});

it("applySnesMouseDelta does nothing when no mouse active", () => {
    const { snes, calls } = makeSnesStub();
    applySnesMouseDelta(snes, 1, 5, 5);
    expect(calls.addMouseDelta).toEqual([]);
});

// ---------------------------------------------------------------------------
// applySnesMouseButton
// ---------------------------------------------------------------------------

describe("applySnesMouseButton", () => {
    it("maps left click (button 0) to set_mouse_left_button", () => {
        const { snes, calls } = makeSnesStub({ mousePorts: [1] });
        applySnesMouseButton(snes, 1, 0, true);
        expect(calls.setMouseLeftButton).toEqual([{ port: 1, pressed: true }]);
        expect(calls.setMouseRightButton).toEqual([]);
    });

    it("maps right click (button 2) to set_mouse_right_button", () => {
        const { snes, calls } = makeSnesStub({ mousePorts: [1] });
        applySnesMouseButton(snes, 1, 2, false);
        expect(calls.setMouseRightButton).toEqual([{ port: 1, pressed: false }]);
        expect(calls.setMouseLeftButton).toEqual([]);
    });

    it("ignores middle click (button 1)", () => {
        const { snes, calls } = makeSnesStub({ mousePorts: [1] });
        applySnesMouseButton(snes, 1, 1, true);
        expect(calls.setMouseLeftButton).toEqual([]);
        expect(calls.setMouseRightButton).toEqual([]);
    });

    it("does nothing when mouse not on port", () => {
        const { snes, calls } = makeSnesStub({ mousePorts: [2] });
        applySnesMouseButton(snes, 1, 0, true);
        expect(calls.setMouseLeftButton).toEqual([]);
    });
});

// ---------------------------------------------------------------------------
// mapSnesScreenX / mapSnesScreenY
// ---------------------------------------------------------------------------

it("mapSnesScreenX maps left edge to 0", () => {
    expect(mapSnesScreenX(0, 256)).toBe(0);
});

it("mapSnesScreenX maps right edge to 255", () => {
    expect(mapSnesScreenX(255, 256)).toBe(255);
});

it("mapSnesScreenX maps center correctly", () => {
    const x = mapSnesScreenX(128, 256);
    expect(x).toBeGreaterThanOrEqual(127);
    expect(x).toBeLessThanOrEqual(129);
});

it("mapSnesScreenX clamps negative values", () => {
    expect(mapSnesScreenX(-10, 256)).toBe(0);
});

it("mapSnesScreenX clamps values beyond width", () => {
    expect(mapSnesScreenX(300, 256)).toBe(255);
});

it("mapSnesScreenY maps top edge to 0", () => {
    expect(mapSnesScreenY(0, 224)).toBe(0);
});

it("mapSnesScreenY maps bottom edge to 223", () => {
    expect(mapSnesScreenY(223, 224)).toBe(223);
});

it("mapSnesScreenY clamps beyond height", () => {
    expect(mapSnesScreenY(300, 224)).toBe(223);
});

// ---------------------------------------------------------------------------
// applySnesSuperScopePosition
// ---------------------------------------------------------------------------

it("applySnesSuperScopePosition maps canvas position to SNES coordinates", () => {
    const { snes, calls } = makeSnesStub({ superScopePorts: [2] });
    // canvas 512x448, position at center → should map to ~128, ~112
    applySnesSuperScopePosition(snes, 2, 256, 224, 512, 448);
    expect(calls.setSuperScopePosition).toHaveLength(1);
    expect(calls.setSuperScopePosition[0].port).toBe(2);
    // Allow ±2 for rounding
    expect(calls.setSuperScopePosition[0].x).toBeGreaterThanOrEqual(126);
    expect(calls.setSuperScopePosition[0].x).toBeLessThanOrEqual(130);
    expect(calls.setSuperScopePosition[0].y).toBeGreaterThanOrEqual(110);
    expect(calls.setSuperScopePosition[0].y).toBeLessThanOrEqual(114);
});

it("applySnesSuperScopePosition does nothing when superscope not on port", () => {
    const { snes, calls } = makeSnesStub({ superScopePorts: [2] });
    applySnesSuperScopePosition(snes, 1, 100, 100, 512, 448);
    expect(calls.setSuperScopePosition).toHaveLength(0);
});

// ---------------------------------------------------------------------------
// applySnesSuperScopeButton
// ---------------------------------------------------------------------------

describe("applySnesSuperScopeButton", () => {
    it("maps left click (button 0) to trigger on superscope port", () => {
        const { snes, calls } = makeSnesStub({ superScopePorts: [2] });
        applySnesSuperScopeButton(snes, 2, 0, true);
        expect(calls.setSuperScopeTrigger).toEqual([{ port: 2, pressed: true }]);
    });

    it("maps right click (button 2) to cursor on superscope port", () => {
        const { snes, calls } = makeSnesStub({ superScopePorts: [2] });
        applySnesSuperScopeButton(snes, 2, 2, true);
        expect(calls.setSuperScopeCursor).toEqual([{ port: 2, pressed: true }]);
    });

    it("does nothing when superscope not on port", () => {
        const { snes, calls } = makeSnesStub({ superScopePorts: [2] });
        applySnesSuperScopeButton(snes, 1, 0, true);
        expect(calls.setSuperScopeTrigger).toEqual([]);
    });
});

// ---------------------------------------------------------------------------
// shouldSuppressSnesJoypadInput
// ---------------------------------------------------------------------------

it("shouldSuppressSnesJoypadInput suppresses port when mouse active", () => {
    const { snes } = makeSnesStub({ mousePorts: [1] });
    expect(shouldSuppressSnesJoypadInput(snes, 1)).toBe(true);
});

it("shouldSuppressSnesJoypadInput suppresses port when superscope active", () => {
    const { snes } = makeSnesStub({ superScopePorts: [2] });
    expect(shouldSuppressSnesJoypadInput(snes, 2)).toBe(true);
});

it("shouldSuppressSnesJoypadInput allows port 2 when mouse only on port 1", () => {
    const { snes } = makeSnesStub({ mousePorts: [1] });
    expect(shouldSuppressSnesJoypadInput(snes, 2)).toBe(false);
});

it("shouldSuppressSnesJoypadInput allows all ports when nothing special active", () => {
    const { snes } = makeSnesStub();
    expect(shouldSuppressSnesJoypadInput(snes, 1)).toBe(false);
    expect(shouldSuppressSnesJoypadInput(snes, 2)).toBe(false);
});

it("shouldSuppressSnesJoypadInput does not suppress multitap ports (joypad use them)", () => {
    const { snes } = makeSnesStub({ multitapPorts: [2] });
    expect(shouldSuppressSnesJoypadInput(snes, 2)).toBe(false);
    expect(shouldSuppressSnesJoypadInput(snes, 3)).toBe(false);
    expect(shouldSuppressSnesJoypadInput(snes, 4)).toBe(false);
});
