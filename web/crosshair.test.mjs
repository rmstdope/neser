import test from "node:test";
import assert from "node:assert/strict";

// Mock DOM environment for testing
function createMockCanvas() {
    let lastArc = null;
    const mockContext = {
        clearRect: () => {},
        strokeStyle: "",
        fillStyle: "",
        lineWidth: 0,
        lineCap: "",
        beginPath: () => {},
        moveTo: () => {},
        lineTo: () => {},
        stroke: () => {},
        arc: (x, y) => {
            lastArc = { x, y };
        },
        fill: () => {},
    };
    
    const mockCanvas = {
        width: 256,
        height: 240,
        style: {
            position: "",
            top: "",
            left: "",
            pointerEvents: "",
            zIndex: "",
            width: "256px",
            height: "240px",
        },
        getContext: () => mockContext,
        remove: () => {},
        parentElement: {
            style: {
                position: "",
            },
            appendChild: () => {},
        },
    };
    
    return { mockCanvas, mockContext, getLastArc: () => lastArc };
}

// Mock document.createElement for crosshair module
const originalDocument = globalThis.document;
const originalWindow = globalThis.window;
let lastOverlayArc = null;

function setupMockDOM() {
    globalThis.document = {
        createElement: (tag) => {
            if (tag === "canvas") {
                const { mockCanvas, getLastArc } = createMockCanvas();
                lastOverlayArc = getLastArc;
                return mockCanvas;
            }
            return {};
        },
    };
    
    globalThis.window = {
        devicePixelRatio: 1,
    };
}

function teardownMockDOM() {
    globalThis.document = originalDocument;
    globalThis.window = originalWindow;
    lastOverlayArc = null;
}

test("createCrosshair initially not visible", async () => {
    setupMockDOM();
    const { createCrosshair } = await import("./crosshair.js");
    
    const { mockCanvas } = createMockCanvas();
    const crosshair = createCrosshair(mockCanvas);
    
    assert.equal(crosshair.visible, false);
    
    crosshair.destroy();
    teardownMockDOM();
});

test("createCrosshair show makes crosshair visible", async () => {
    setupMockDOM();
    const { createCrosshair } = await import("./crosshair.js");
    
    const { mockCanvas } = createMockCanvas();
    const crosshair = createCrosshair(mockCanvas);
    
    crosshair.show();
    assert.equal(crosshair.visible, true);
    
    crosshair.destroy();
    teardownMockDOM();
});

test("createCrosshair hide makes crosshair invisible", async () => {
    setupMockDOM();
    const { createCrosshair } = await import("./crosshair.js");
    
    const { mockCanvas } = createMockCanvas();
    const crosshair = createCrosshair(mockCanvas);
    
    crosshair.show();
    assert.equal(crosshair.visible, true);
    
    crosshair.hide();
    assert.equal(crosshair.visible, false);
    
    crosshair.destroy();
    teardownMockDOM();
});

test("createCrosshair updatePosition can be called", async () => {
    setupMockDOM();
    const { createCrosshair } = await import("./crosshair.js");
    
    const { mockCanvas } = createMockCanvas();
    const crosshair = createCrosshair(mockCanvas);
    
    // Should not throw
    crosshair.updatePosition(100, 100);
    
    crosshair.destroy();
    teardownMockDOM();
});

test("createCrosshair clamps position to bounds", async () => {
    setupMockDOM();
    const { createCrosshair } = await import("./crosshair.js");

    const { mockCanvas } = createMockCanvas();
    const crosshair = createCrosshair(mockCanvas);

    crosshair.show();
    crosshair.updatePosition(-10, 300);

    const arc = lastOverlayArc ? lastOverlayArc() : null;
    assert.ok(arc, "Expected crosshair to draw the center dot");
    assert.equal(arc.x, 0);
    assert.equal(arc.y, 239);

    crosshair.destroy();
    teardownMockDOM();
});

