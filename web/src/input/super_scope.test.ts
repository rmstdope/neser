import { describe, expect, it } from "vitest";
import {
    SUPER_SCOPE_CONNECTED,
    SUPER_SCOPE_MOUSE_RELEASED,
    createSuperScopeSession,
    superScopeKeyAction,
    superScopeTurboMessage,
} from "./super_scope";

describe("Super Scope words", () => {
    it("are the agreed ones, matching src/snes/frontend_toasts.rs", () => {
        expect(SUPER_SCOPE_CONNECTED).toBe("Super Scope connected — click to aim with the mouse");
        expect(SUPER_SCOPE_MOUSE_RELEASED).toBe("Mouse released — click the game to aim again");
        expect(superScopeTurboMessage(true)).toBe("Turbo on");
        expect(superScopeTurboMessage(false)).toBe("Turbo off");
    });
});

describe("createSuperScopeSession", () => {
    it("fires on the first capturing click and aims where it was clicked", () => {
        const session = createSuperScopeSession();
        expect(session.mouseDown(0, false, 40, 30)).toEqual({ requestLock: true, forward: true });
        expect(session.position()).toEqual({ x: 40, y: 30 });
    });

    it("forwards the right button (cursor) as a capturing click too", () => {
        const session = createSuperScopeSession();
        expect(session.mouseDown(2, false, 1, 1)).toEqual({ requestLock: true, forward: true });
    });

    it("ignores other buttons", () => {
        const session = createSuperScopeSession();
        expect(session.mouseDown(1, false, 1, 1)).toEqual({ requestLock: false, forward: false });
    });

    it("gives back a lock it did not ask for, so loading a game never captures the mouse", () => {
        const session = createSuperScopeSession();
        expect(session.lockChanged(true)).toEqual({ toast: null, release: true });
        expect(session.captured()).toBe(false);
        expect(session.lockChanged(false)).toEqual({ toast: null, release: false });
    });

    it("forwards clicks while captured", () => {
        const session = createSuperScopeSession();
        session.mouseDown(0, false, 0, 0);
        session.lockChanged(true);
        expect(session.mouseDown(0, true, 0, 0)).toEqual({ requestLock: false, forward: true });
    });

    it("says the mouse was released when a capture ends, and recaptures without firing", () => {
        const session = createSuperScopeSession();
        session.mouseDown(0, false, 10, 10);
        session.lockChanged(true);
        expect(session.lockChanged(false)).toEqual({ toast: SUPER_SCOPE_MOUSE_RELEASED, release: false });
        expect(session.mouseDown(0, false, 10, 10)).toEqual({ requestLock: true, forward: false });
        session.lockChanged(true);
        expect(session.mouseDown(0, true, 0, 0)).toEqual({ requestLock: false, forward: true });
    });

    it("still recaptures without firing when the browser refuses a lock", () => {
        const session = createSuperScopeSession();
        session.mouseDown(0, false, 0, 0);
        session.lockChanged(true);
        session.lockChanged(false);
        expect(session.mouseDown(0, false, 0, 0).forward).toBe(false);
        session.lockRefused();
        expect(session.mouseDown(0, false, 0, 0)).toEqual({ requestLock: true, forward: false });
        session.lockRefused();
        // A lock the page asks for later is still not taken as the scope's capture.
        expect(session.lockChanged(true)).toEqual({ toast: null, release: true });
    });

    it("says nothing when the lock is lost before it was ever captured", () => {
        const session = createSuperScopeSession();
        expect(session.lockChanged(false)).toEqual({ toast: null, release: false });
        expect(session.mouseDown(0, false, 0, 0).forward).toBe(true);
    });

    it("moves the aim by the pointer's movement, kept inside the picture", () => {
        const session = createSuperScopeSession();
        session.mouseDown(0, false, 100, 50);
        session.lockChanged(true);
        expect(session.move(20, -10, 512, 448)).toEqual({ x: 120, y: 40 });
        expect(session.move(-1000, 1000, 512, 448)).toEqual({ x: 0, y: 447 });
    });

    it("reports whether the mouse is captured", () => {
        const session = createSuperScopeSession();
        expect(session.captured()).toBe(false);
        session.mouseDown(0, false, 0, 0);
        session.lockChanged(true);
        expect(session.captured()).toBe(true);
        session.lockChanged(false);
        expect(session.captured()).toBe(false);
    });
});

describe("superScopeKeyAction", () => {
    it("makes the Select key the Turbo switch and the Start key Pause", () => {
        expect(superScopeKeyAction("4")).toBe("turbo");
        expect(superScopeKeyAction("5")).toBe("pause");
        expect(superScopeKeyAction("w")).toBeNull();
    });
});
