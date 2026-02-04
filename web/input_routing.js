/**
 * Input routing logic for multiple controllers.
 * 
 * This module handles the routing of keyboard, gamepad, and paddle inputs
 * to the appropriate NES controller ports based on the number of connected
 * gamepads and the presence of paddle controllers.
 */

/**
 * Select up to two connected gamepads from the gamepads array.
 * 
 * @param {Gamepad[]} gamepads - Array of gamepads (may contain null/undefined)
 * @returns {Gamepad[]} Array of connected gamepads (0-2 elements)
 */
export function selectGamepads(gamepads) {
    if (!gamepads) return [];
    
    const connected = [];
    for (const gamepad of gamepads) {
        if (gamepad && gamepad.connected) {
            connected.push(gamepad);
            if (connected.length >= 2) {
                break;
            }
        }
    }
    return connected;
}

/**
 * Determine which controller port the keyboard should control.
 * 
 * Rules:
 * - No gamepads: keyboard controls controller 1
 * - One gamepad: keyboard controls controller 2
 * - Two or more gamepads: keyboard is disabled (returns null)
 * 
 * @param {number} gamepadCount - Number of connected gamepads
 * @returns {number|null} Controller number (1 or 2) or null if keyboard disabled
 */
export function getKeyboardControllerTarget(gamepadCount) {
    if (gamepadCount === 0) {
        return 1;
    } else if (gamepadCount === 1) {
        return 2;
    } else {
        return null; // Two or more gamepads, keyboard disabled
    }
}

/**
 * Check if joypad input should be suppressed for a given controller.
 * 
 * Joypad input is suppressed on a port if that port has a paddle controller.
 * 
 * @param {Object} nes - NES emulator instance
 * @param {number} controller - Controller number (1 or 2)
 * @returns {boolean} True if joypad input should be suppressed
 */
export function shouldSuppressJoypadInput(nes, controller) {
    const paddlePort = nes.paddle_port();
    return paddlePort === controller;
}
