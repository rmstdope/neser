import { describe, expect, it } from "vitest";
import {
    SNES_MOUSE_RELEASED,
    activeCaptureSession,
    createSnesMouseSession,
} from "./snes_mouse";
import { createSuperScopeSession } from "./super_scope";

describe("activeCaptureSession", () => {
    const scope = createSuperScopeSession();
    const mouse = createSnesMouseSession();

    it("is the scope's when a scope is plugged in, even beside a mouse", () => {
        expect(activeCaptureSession(true, true, scope, mouse)).toBe(scope);
        expect(activeCaptureSession(true, false, scope, mouse)).toBe(scope);
    });

    it("is the mouse's with only a mouse, and none without either", () => {
        expect(activeCaptureSession(false, true, scope, mouse)).toBe(mouse);
        expect(activeCaptureSession(false, false, scope, mouse)).toBeNull();
    });
});

describe("SNES Mouse words", () => {
    it("are the agreed ones, matching src/snes/frontend_toasts.rs", () => {
        expect(SNES_MOUSE_RELEASED).toBe("Mouse released — click the game to use it again");
    });
});

describe("createSnesMouseSession", () => {
    it("captures on the first click without the click reaching the game", () => {
        const session = createSnesMouseSession();
        expect(session.mouseDown(0, false, 40, 30)).toEqual({ requestLock: true, forward: false });
        expect(session.mouseDown(2, false, 40, 30)).toEqual({ requestLock: true, forward: false });
    });

    it("forwards clicks once the mouse is captured", () => {
        const session = createSnesMouseSession();
        session.mouseDown(0, false, 0, 0);
        session.lockChanged(true);
        expect(session.captured()).toBe(true);
        expect(session.mouseDown(0, true, 0, 0)).toEqual({ requestLock: false, forward: true });
        expect(session.mouseDown(2, true, 0, 0)).toEqual({ requestLock: false, forward: true });
    });

    it("says the mouse was released only after a capture, and recaptures without a click", () => {
        const session = createSnesMouseSession();
        expect(session.lockChanged(false)).toEqual({ toast: null, release: false });
        session.mouseDown(0, false, 0, 0);
        session.lockChanged(true);
        expect(session.lockChanged(false)).toEqual({ toast: SNES_MOUSE_RELEASED, release: false });
        expect(session.captured()).toBe(false);
        expect(session.mouseDown(0, false, 0, 0)).toEqual({ requestLock: true, forward: false });
    });

    it("gives back a lock it did not ask for, so loading a game never captures the mouse", () => {
        const session = createSnesMouseSession();
        expect(session.lockChanged(true)).toEqual({ toast: null, release: true });
        expect(session.captured()).toBe(false);
    });

    it("ignores the middle button", () => {
        const session = createSnesMouseSession();
        expect(session.mouseDown(1, false, 0, 0)).toEqual({ requestLock: false, forward: false });
    });
});
