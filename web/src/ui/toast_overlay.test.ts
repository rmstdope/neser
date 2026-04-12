import { expect, it } from "vitest";
import { createToastOverlay, createToastContainer, drainNesToasts } from "./toast_overlay";

function createMockContainer() {
    const children: any[] = [];
    return {
        children,
        appendChild(node: any) {
            children.push(node);
        },
        removeChild(node: any) {
            const index = children.indexOf(node);
            if (index >= 0) {
                children.splice(index, 1);
            }
        }
    };
}

it("createToastContainer mounts toast host in provided element", () => {
    const host = {
        children: [] as any[],
        appendChild(node: any) {
            this.children.push(node);
        }
    };
    const originalDocument = globalThis.document;
    globalThis.document = {
        createElement() {
            return { className: "", children: [] };
        }
    } as any;

    const container = createToastContainer(host as any);

    globalThis.document = originalDocument;
    expect(container.className).toBe("neser-toast-container");
    expect(host.children.length).toBe(1);
    expect(host.children[0]).toBe(container);
});

it("createToastOverlay show appends and auto-removes toast", () => {
    const container = createMockContainer();
    let scheduledCallback: (() => void) | null = null;
    let scheduledDelay = 0;

    const overlay = createToastOverlay({
        container,
        createNode: (message: string) => ({ textContent: message }),
        schedule: (callback: () => void, delayMs: number) => {
            scheduledCallback = callback;
            scheduledDelay = delayMs;
            return 1;
        },
        durationMs: 1200
    } as any);

    overlay.show("Cartridge loaded: mario.nes");

    expect(container.children.length).toBe(1);
    expect(container.children[0].textContent).toBe("Cartridge loaded: mario.nes");
    expect(scheduledDelay).toBe(1200);

    scheduledCallback!();
    expect(container.children.length).toBe(0);
});

it("createToastOverlay showMany preserves order", () => {
    const container = createMockContainer();

    const overlay = createToastOverlay({
        container,
        createNode: (message: string) => ({ textContent: message }),
        schedule: () => 1,
        durationMs: 1
    } as any);

    overlay.showMany([
        "Gamepad found: using 1 gamepad",
        "Cartridge loaded: mario.nes",
        "Emulator timing: PAL"
    ]);

    expect(
        container.children.map((node: any) => node.textContent)
    ).toEqual([
        "Gamepad found: using 1 gamepad",
        "Cartridge loaded: mario.nes",
        "Emulator timing: PAL"
    ]);
});

it("drainNesToasts forwards drained messages to overlay", () => {
    const seen: string[] = [];
    const overlay = {
        showMany(messages: string[]) {
            seen.push(...messages);
        }
    };
    const nes = {
        drain_toasts() {
            return ["Cartridge load failed: bad.nes", { toString: () => "Emulator timing: PAL" }];
        }
    };

    drainNesToasts(nes as any, overlay as any);

    expect(seen).toEqual(["Cartridge load failed: bad.nes", "Emulator timing: PAL"]);
});

it("drainNesToasts is a no-op when nes is null", () => {
    let called = false;
    const overlay = {
        showMany(_messages: string[]) {
            called = true;
        }
    };

    drainNesToasts(null, overlay as any);

    expect(called).toBe(false);
});

it("drainNesToasts is a no-op when nes is undefined", () => {
    let called = false;
    const overlay = {
        showMany(_messages: string[]) {
            called = true;
        }
    };

    // Intentionally pass undefined NES instance
    drainNesToasts(undefined as any, overlay as any);

    expect(called).toBe(false);
});
