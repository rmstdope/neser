/**
 * @vitest-environment jsdom
 */
import { describe, expect, it, vi, beforeEach, afterEach, Mock } from "vitest";

import {
    isTouchDevice,
    isHandheldDevice,
    resolveButtonFromElement,
    TouchInputManager,
    NES_BUTTON,
} from "./touch_controls";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Create a minimal DOM element with a data-button attribute. */
function makeButtonElement(button: string): Element {
    const el = document.createElement("div");
    el.setAttribute("data-button", button);
    return el;
}

/** Build a container that houses several button elements and can resolve
 *  elementFromPoint queries. */
function makeContainer(buttons: Record<string, Element>) {
    const container = document.createElement("div");
    for (const el of Object.values(buttons)) {
        container.appendChild(el);
    }
    return container;
}

/** Create a fake Touch object with the given identifier and target element. */
function fakeTouch(identifier: number, target: Element, x = 0, y = 0): Touch {
    return {
        identifier,
        target,
        clientX: x,
        clientY: y,
        pageX: x,
        pageY: y,
        screenX: x,
        screenY: y,
        radiusX: 0,
        radiusY: 0,
        rotationAngle: 0,
        force: 0,
    };
}

// ---------------------------------------------------------------------------
// isTouchDevice
// ---------------------------------------------------------------------------

describe("isTouchDevice", () => {
    it("returns true when ontouchstart is in window", () => {
        // Arrange: simulate a touch-capable browser
        Object.defineProperty(window, "ontouchstart", {
            value: null,
            writable: true,
            configurable: true,
        });

        // Act & Assert
        expect(isTouchDevice()).toBe(true);

        // Cleanup
        delete (window as any).ontouchstart;
    });

    it("returns true when navigator.maxTouchPoints > 0", () => {
        Object.defineProperty(navigator, "maxTouchPoints", {
            value: 5,
            writable: true,
            configurable: true,
        });

        expect(isTouchDevice()).toBe(true);

        Object.defineProperty(navigator, "maxTouchPoints", {
            value: 0,
            writable: true,
            configurable: true,
        });
    });

    it("returns false on a desktop browser", () => {
        // Ensure neither touch indicator is present
        delete (window as any).ontouchstart;
        Object.defineProperty(navigator, "maxTouchPoints", {
            value: 0,
            writable: true,
            configurable: true,
        });

        expect(isTouchDevice()).toBe(false);
    });
});

// ---------------------------------------------------------------------------
// isHandheldDevice
// ---------------------------------------------------------------------------

/** Helper: install a mock window.matchMedia that matches `(pointer: coarse)`. */
function mockMatchMedia(matchesPointerCoarse: boolean) {
    vi.stubGlobal(
        "matchMedia",
        vi.fn((query: string) => ({
            matches: query === "(pointer: coarse)" ? matchesPointerCoarse : false,
            media: query,
            onchange: null,
            addListener: vi.fn(),
            removeListener: vi.fn(),
            addEventListener: vi.fn(),
            removeEventListener: vi.fn(),
            dispatchEvent: vi.fn(),
        })),
    );
}

describe("isHandheldDevice", () => {
    beforeEach(() => {
        // Default to a large desktop viewport
        vi.stubGlobal("innerWidth", 1920);
        vi.stubGlobal("innerHeight", 1080);
    });

    afterEach(() => {
        vi.unstubAllGlobals();
    });

    it("returns true when pointer:coarse and small portrait viewport", () => {
        mockMatchMedia(true);
        vi.stubGlobal("innerWidth", 390);
        vi.stubGlobal("innerHeight", 844);
        expect(isHandheldDevice()).toBe(true);
    });

    it("returns true in landscape orientation where min dimension is still small", () => {
        mockMatchMedia(true);
        vi.stubGlobal("innerWidth", 844);
        vi.stubGlobal("innerHeight", 390);
        expect(isHandheldDevice()).toBe(true);
    });

    it("returns false when pointer:coarse but viewport is too large", () => {
        mockMatchMedia(true);
        // innerWidth/innerHeight set to 1920x1080 by beforeEach
        expect(isHandheldDevice()).toBe(false);
    });

    it("returns false when not pointer:coarse even with small viewport", () => {
        mockMatchMedia(false);
        vi.stubGlobal("innerWidth", 390);
        vi.stubGlobal("innerHeight", 844);
        expect(isHandheldDevice()).toBe(false);
    });

    it("returns false when matchMedia is not available", () => {
        vi.stubGlobal("matchMedia", undefined);
        vi.stubGlobal("innerWidth", 390);
        vi.stubGlobal("innerHeight", 844);
        expect(isHandheldDevice()).toBe(false);
    });
});

// ---------------------------------------------------------------------------
// resolveButtonFromElement
// ---------------------------------------------------------------------------

describe("resolveButtonFromElement", () => {
    it("returns the button number for element with data-button='a'", () => {
        const el = makeButtonElement("a");
        expect(resolveButtonFromElement(el)).toBe(NES_BUTTON.A);
    });

    it("returns the button number for element with data-button='b'", () => {
        const el = makeButtonElement("b");
        expect(resolveButtonFromElement(el)).toBe(NES_BUTTON.B);
    });

    it("returns the button number for each d-pad direction", () => {
        expect(resolveButtonFromElement(makeButtonElement("up"))).toBe(NES_BUTTON.UP);
        expect(resolveButtonFromElement(makeButtonElement("down"))).toBe(NES_BUTTON.DOWN);
        expect(resolveButtonFromElement(makeButtonElement("left"))).toBe(NES_BUTTON.LEFT);
        expect(resolveButtonFromElement(makeButtonElement("right"))).toBe(NES_BUTTON.RIGHT);
    });

    it("returns the button number for start and select", () => {
        expect(resolveButtonFromElement(makeButtonElement("start"))).toBe(NES_BUTTON.START);
        expect(resolveButtonFromElement(makeButtonElement("select"))).toBe(NES_BUTTON.SELECT);
    });

    it("returns undefined for element without data-button", () => {
        const el = document.createElement("div");
        expect(resolveButtonFromElement(el)).toBeUndefined();
    });

    it("returns undefined for null element", () => {
        expect(resolveButtonFromElement(null)).toBeUndefined();
    });
});

// ---------------------------------------------------------------------------
// TouchInputManager — basic press/release
// ---------------------------------------------------------------------------

describe("TouchInputManager", () => {
    let callback: Mock<(button: number, pressed: boolean) => void>;
    let manager: TouchInputManager;
    let container: HTMLElement;
    let btnA: Element;
    let btnB: Element;
    let btnUp: Element;
    let btnRight: Element;

    beforeEach(() => {
        callback = vi.fn();
        manager = new TouchInputManager(callback);

        btnA = makeButtonElement("a");
        btnB = makeButtonElement("b");
        btnUp = makeButtonElement("up");
        btnRight = makeButtonElement("right");
        container = makeContainer({
            a: btnA,
            b: btnB,
            up: btnUp,
            right: btnRight,
        }) as HTMLElement;
    });

    it("fires pressed callback when touch starts on a button", () => {
        const touch = fakeTouch(0, btnA);
        manager.handleTouchStart(touch);

        expect(callback).toHaveBeenCalledWith(NES_BUTTON.A, true);
        expect(manager.activeCount).toBe(1);
    });

    it("fires released callback when touch ends", () => {
        const touch = fakeTouch(0, btnA);
        manager.handleTouchStart(touch);
        callback.mockClear();

        manager.handleTouchEnd(touch);

        expect(callback).toHaveBeenCalledWith(NES_BUTTON.A, false);
        expect(manager.activeCount).toBe(0);
    });

    it("fires released callback on touchcancel", () => {
        const touch = fakeTouch(0, btnA);
        manager.handleTouchStart(touch);
        callback.mockClear();

        manager.handleTouchCancel(touch);

        expect(callback).toHaveBeenCalledWith(NES_BUTTON.A, false);
        expect(manager.activeCount).toBe(0);
    });

    it("does not fire callback when touch starts outside any button", () => {
        const outsideEl = document.createElement("div");
        container.appendChild(outsideEl);
        const touch = fakeTouch(0, outsideEl);

        manager.handleTouchStart(touch);

        expect(callback).not.toHaveBeenCalled();
        expect(manager.activeCount).toBe(0);
    });

    it("tracks the active button for each touch identifier", () => {
        manager.handleTouchStart(fakeTouch(0, btnA));
        manager.handleTouchStart(fakeTouch(1, btnB));

        expect(manager.getButtonForTouch(0)).toBe(NES_BUTTON.A);
        expect(manager.getButtonForTouch(1)).toBe(NES_BUTTON.B);
        expect(manager.activeCount).toBe(2);
    });

    // -----------------------------------------------------------------------
    // Multi-touch — simultaneous button presses
    // -----------------------------------------------------------------------

    it("supports simultaneous presses on different buttons", () => {
        manager.handleTouchStart(fakeTouch(0, btnRight));
        manager.handleTouchStart(fakeTouch(1, btnA));

        expect(callback).toHaveBeenCalledWith(NES_BUTTON.RIGHT, true);
        expect(callback).toHaveBeenCalledWith(NES_BUTTON.A, true);
        expect(manager.activeCount).toBe(2);
    });

    it("releases only the specific touch that ended", () => {
        manager.handleTouchStart(fakeTouch(0, btnRight));
        manager.handleTouchStart(fakeTouch(1, btnA));
        callback.mockClear();

        manager.handleTouchEnd(fakeTouch(0, btnRight));

        expect(callback).toHaveBeenCalledTimes(1);
        expect(callback).toHaveBeenCalledWith(NES_BUTTON.RIGHT, false);
        expect(manager.activeCount).toBe(1);
        expect(manager.getButtonForTouch(1)).toBe(NES_BUTTON.A);
    });

    // -----------------------------------------------------------------------
    // Touch move — sliding between buttons
    // -----------------------------------------------------------------------

    it("fires release old + press new when finger slides to another button", () => {
        manager.handleTouchStart(fakeTouch(0, btnUp));
        callback.mockClear();

        // Simulate finger sliding to the Right button
        manager.handleTouchMove(fakeTouch(0, btnRight));

        expect(callback).toHaveBeenCalledWith(NES_BUTTON.UP, false);
        expect(callback).toHaveBeenCalledWith(NES_BUTTON.RIGHT, true);
        expect(manager.getButtonForTouch(0)).toBe(NES_BUTTON.RIGHT);
    });

    it("does not fire callback when finger stays on the same button during move", () => {
        manager.handleTouchStart(fakeTouch(0, btnA));
        callback.mockClear();

        manager.handleTouchMove(fakeTouch(0, btnA));

        expect(callback).not.toHaveBeenCalled();
    });

    it("releases button when finger slides off all buttons", () => {
        manager.handleTouchStart(fakeTouch(0, btnA));
        callback.mockClear();

        const outsideEl = document.createElement("div");
        container.appendChild(outsideEl);
        manager.handleTouchMove(fakeTouch(0, outsideEl));

        expect(callback).toHaveBeenCalledWith(NES_BUTTON.A, false);
        expect(manager.activeCount).toBe(0);
    });

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    it("ignores duplicate touchend for the same identifier", () => {
        const touch = fakeTouch(0, btnA);
        manager.handleTouchStart(touch);
        manager.handleTouchEnd(touch);
        callback.mockClear();

        manager.handleTouchEnd(touch);

        expect(callback).not.toHaveBeenCalled();
    });

    it("ignores touchend for unknown identifier", () => {
        manager.handleTouchEnd(fakeTouch(99, btnA));
        expect(callback).not.toHaveBeenCalled();
    });
});
