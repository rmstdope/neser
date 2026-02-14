import assert from "node:assert/strict";
import test from "node:test";
import { createToastOverlay, createToastContainer, drainNesToasts } from "./toast_overlay.js";

function createMockContainer() {
    const children = [];
    return {
        children,
        appendChild(node) {
            children.push(node);
        },
        removeChild(node) {
            const index = children.indexOf(node);
            if (index >= 0) {
                children.splice(index, 1);
            }
        }
    };
}

test("createToastContainer mounts toast host in provided element", () => {
    const host = {
        children: [],
        appendChild(node) {
            this.children.push(node);
        }
    };
    const originalDocument = globalThis.document;
    globalThis.document = {
        createElement() {
            return { className: "", children: [] };
        }
    };

    const container = createToastContainer(host);

    globalThis.document = originalDocument;
    assert.equal(container.className, "neser-toast-container");
    assert.equal(host.children.length, 1);
    assert.equal(host.children[0], container);
});

test("createToastOverlay show appends and auto-removes toast", () => {
    const container = createMockContainer();
    let scheduledCallback = null;
    let scheduledDelay = 0;

    const overlay = createToastOverlay({
        container,
        createNode: (message) => ({ textContent: message }),
        schedule: (callback, delayMs) => {
            scheduledCallback = callback;
            scheduledDelay = delayMs;
            return 1;
        },
        durationMs: 1200
    });

    overlay.show("Cartridge loaded: mario.nes");

    assert.equal(container.children.length, 1);
    assert.equal(container.children[0].textContent, "Cartridge loaded: mario.nes");
    assert.equal(scheduledDelay, 1200);

    scheduledCallback();
    assert.equal(container.children.length, 0);
});

test("createToastOverlay showMany preserves order", () => {
    const container = createMockContainer();

    const overlay = createToastOverlay({
        container,
        createNode: (message) => ({ textContent: message }),
        schedule: () => 1,
        durationMs: 1
    });

    overlay.showMany([
        "Gamepad found: using 1 gamepad",
        "Cartridge loaded: mario.nes",
        "Emulator timing: PAL"
    ]);

    assert.deepEqual(
        container.children.map((node) => node.textContent),
        [
            "Gamepad found: using 1 gamepad",
            "Cartridge loaded: mario.nes",
            "Emulator timing: PAL"
        ]
    );
});

test("drainNesToasts forwards drained messages to overlay", () => {
    const seen = [];
    const overlay = {
        showMany(messages) {
            seen.push(...messages);
        }
    };
    const nes = {
        drain_toasts() {
            return ["Cartridge load failed: bad.nes", { toString: () => "Emulator timing: PAL" }];
        }
    };

    drainNesToasts(nes, overlay);

    assert.deepEqual(seen, ["Cartridge load failed: bad.nes", "Emulator timing: PAL"]);
});

test("drainNesToasts is a no-op when nes is null", () => {
    let called = false;
    const overlay = {
        showMany(_messages) {
            called = true;
        }
    };

    drainNesToasts(null, overlay);

    assert.equal(called, false);
});

test("drainNesToasts is a no-op when nes is undefined", () => {
    let called = false;
    const overlay = {
        showMany(_messages) {
            called = true;
        }
    };

    // Intentionally pass undefined NES instance
    drainNesToasts(undefined, overlay);

    assert.equal(called, false);
});
