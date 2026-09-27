/**
 * Playing an SNES Mouse game with the host mouse: a click on the game captures the mouse
 * and never reaches the game (it would paint or select something), Escape or losing the
 * lock releases it. The capture itself is the Super Scope's state machine.
 */
import { createClickCaptureSession, type ClickCaptureSession } from "./super_scope";

/** The agreed words; the same strings live in `src/snes/frontend_toasts.rs`. */
export const SNES_MOUSE_CONNECTED = "SNES Mouse connected — click the game to use the mouse";
export const SNES_MOUSE_RELEASED = "Mouse released — click the game to use it again";

export function createSnesMouseSession() {
    return createClickCaptureSession({
        forwardsCapturingClick: false,
        releasedMessage: SNES_MOUSE_RELEASED,
    });
}

/**
 * The session that captures the mouse for the SNES's devices: the Super Scope's whenever a
 * scope is plugged in (a mouse chosen for the other port follows its capture), otherwise
 * the SNES Mouse's, or none without either.
 */
export function activeCaptureSession(
    hasSuperScope: boolean,
    hasSnesMouse: boolean,
    scopeSession: ClickCaptureSession,
    mouseSession: ClickCaptureSession,
): ClickCaptureSession | null {
    if (hasSuperScope) {
        return scopeSession;
    }
    return hasSnesMouse ? mouseSession : null;
}
