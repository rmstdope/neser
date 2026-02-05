import test from "node:test";
import assert from "node:assert/strict";

// Mock DOM environment for testing
function createMockCanvas() {
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
        arc: () => {},
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
            appendChild: () => {},
        },
    };
    
    return mockCanvas;
}

// Mock document.createElement for crosshair module
const originalDocument = globalThis.document;
const originalWindow = globalThis.window;

function setupMockDOM() {
    globalThis.document = {
        createElement: (tag) => {
            if (tag === "canvas") {
                return createMockCanvas();
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
}

test("createCrosshair initially not visible", async () => {
    setupMockDOM();
    const { createCrosshair } = await import("./crosshair.js");
    
    const targetCanvas = createMockCanvas();
    const crosshair = createCrosshair(targetCanvas);
    
    assert.equal(crosshair.visible, false);
    
    crosshair.destroy();
    teardownMockDOM();
});

test("createCrosshair show makes crosshair visible", async () => {
    setupMockDOM();
    const { createCrosshair } = await import("./crosshair.js");
    
    const targetCanvas = createMockCanvas();
    const crosshair = createCrosshair(targetCanvas);
    
    crosshair.show();
    assert.equal(crosshair.visible, true);
    
    crosshair.destroy();
    teardownMockDOM();
});

test("createCrosshair hide makes crosshair invisible", async () => {
    setupMockDOM();
    const { createCrosshair } = await import("./crosshair.js");
    
    const targetCanvas = createMockCanvas();
    const crosshair = createCrosshair(targetCanvas);
    
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
    
    const targetCanvas = createMockCanvas();
    const crosshair = createCrosshair(targetCanvas);
    
    // Should not throw
    crosshair.updatePosition(100, 100);
    
    crosshair.destroy();
    teardownMockDOM();
});

