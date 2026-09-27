/**
 * Playing a Super Scope game with the mouse: when a click captures the pointer, whether
 * it also fires, where the scope aims while the pointer is locked, and what the player is
 * told. Pure, so `app.ts` only wires it to the DOM and the emulator.
 */

/** The agreed words; the same strings live in `src/snes/frontend_toasts.rs`. */
export const SUPER_SCOPE_CONNECTED = "Super Scope connected — click to aim with the mouse";
export const SUPER_SCOPE_MOUSE_RELEASED = "Mouse released — click the game to aim again";

export function superScopeTurboMessage(on: boolean): string {
    return on ? "Turbo on" : "Turbo off";
}

/** The Select key (4) is the Turbo switch and the Start key (5) is Pause. */
export function superScopeKeyAction(key: string): "turbo" | "pause" | null {
    if (key === "4") {
        return "turbo";
    }
    if (key === "5") {
        return "pause";
    }
    return null;
}

const FIRE_BUTTON = 0;
const CURSOR_BUTTON = 2;

export function createSuperScopeSession() {
    let isCaptured = false;
    let lockRequested = false;
    let releasedByPlayer = false;
    let x = 0;
    let y = 0;

    return {
        /**
         * A button pressed on the game at canvas position (`atX`, `atY`). Without the lock
         * it asks for one; the press is forwarded to the scope unless the mouse was released
         * (a recapturing click never fires).
         */
        mouseDown(button: number, locked: boolean, atX: number, atY: number) {
            if (button !== FIRE_BUTTON && button !== CURSOR_BUTTON) {
                return { requestLock: false, forward: false };
            }
            if (locked) {
                return { requestLock: false, forward: true };
            }
            x = atX;
            y = atY;
            const forward = !releasedByPlayer;
            releasedByPlayer = false;
            lockRequested = true;
            return { requestLock: true, forward };
        },

        /**
         * The pointer lock was gained or lost (Escape, focus loss, anything else). A lock
         * this session did not ask for (the page asks for one when a game is chosen) is to
         * be given back (`release`): only a click on the game captures the mouse.
         */
        lockChanged(locked: boolean): { toast: string | null; release: boolean } {
            if (locked) {
                if (!lockRequested) {
                    return { toast: null, release: true };
                }
                lockRequested = false;
                isCaptured = true;
                return { toast: null, release: false };
            }
            lockRequested = false;
            if (!isCaptured) {
                return { toast: null, release: false };
            }
            isCaptured = false;
            releasedByPlayer = true;
            return { toast: SUPER_SCOPE_MOUSE_RELEASED, release: false };
        },

        /** Pointer movement while locked, kept inside a `width`×`height` picture. */
        move(dx: number, dy: number, width: number, height: number) {
            x = Math.min(Math.max(x + dx, 0), Math.max(0, width - 1));
            y = Math.min(Math.max(y + dy, 0), Math.max(0, height - 1));
            return { x, y };
        },

        position() {
            return { x, y };
        },

        captured() {
            return isCaptured;
        },
    };
}

export type SuperScopeSession = ReturnType<typeof createSuperScopeSession>;
