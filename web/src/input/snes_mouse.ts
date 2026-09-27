/**
 * Playing an SNES Mouse game with the host mouse: a click on the game captures the mouse
 * and never reaches the game (it would paint or select something), Escape or losing the
 * lock releases it. The capture itself is the Super Scope's state machine.
 */
import { createClickCaptureSession } from "./super_scope";

/** The agreed words; the same strings live in `src/snes/frontend_toasts.rs`. */
export const SNES_MOUSE_CONNECTED = "SNES Mouse connected — click the game to use the mouse";
export const SNES_MOUSE_RELEASED = "Mouse released — click the game to use it again";

export function createSnesMouseSession() {
    return createClickCaptureSession({
        forwardsCapturingClick: false,
        releasedMessage: SNES_MOUSE_RELEASED,
    });
}
