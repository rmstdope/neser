import { expect, it } from "vitest";
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

it("createToastContainer mounts toast host in provided element", () => {
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
    expect(container.className).toBe("neser-toast-container");
    expect(host.children.length).toBe(1);
    expect(host.children[0]).toBe(container);
});

it("createToastOverlay show appends and auto-removes toast", () => {
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

    expect(container.children.length).toBe(1);
    expect(container.children[0].textContent).toBe("Cartridge loaded: mario.nes");
    expect(scheduledDelay).toBe(1200);

    scheduledCallback();
    expect(container.children.length).toBe(0);
});

it("createToastOverlay showMany preserves order", () => {
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

    expect(
        container.children.map((node) => node.textContent),
        [
            "Gamepad found: using 1 gamepad",
            "Cartridge loaded: mario.nes",
            "Emulator timing: PAL"
        ]
    );
});

it("drainNesToasts forwards drained messages to overlay", () => {
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

    expect(seen).toEqual(["Cartridge load failed: bad.nes", "Emulator timing: PAL"]);
});

it("drainNesToasts is a no-op when nes is null", () => {
    let called = false;
    const overlay = {
        showMany(_messages) {
            called = true;
        }
    };

    drainNesToasts(null, overlay);

    expect(called).toBe(false);
});

it("drainNesToasts is a no-op when nes is undefined", () => {
    let called = false;
    const overlay = {
        showMany(_messages) {
            called = true;
        }
    };

    // Intentionally pass undefined NES instance
    drainNesToasts(undefined, overlay);

    expect(called).toBe(false);
});
