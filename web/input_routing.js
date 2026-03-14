/**
 * Input routing logic for multiple controllers.
 * 
 * This module handles the routing of keyboard, gamepad, and mouse inputs
 * to the appropriate NES controller ports based on the number of connected
 * gamepads and the presence of mouse-emulated controllers.
 */

/**
 * Determine which controller port(s) the keyboard should control.
 * 
 * Rules:
 * - No gamepads: keyboard controls both controllers (1 and 2)
 * - One gamepad: keyboard controls controller 2
 * - Two or more gamepads: keyboard is disabled (empty array)
 * 
 * @param {number} gamepadCount - Number of connected gamepads
 * @param {boolean} fourScoreEnabled - Whether Four Score mode is enabled
 * @returns {number[]} Array of controller numbers that keyboard should control
 */
export function getKeyboardControllerTarget(gamepadCount, fourScoreEnabled = false) {
    if (fourScoreEnabled) {
        if (gamepadCount === 0) {
            return [1, 2];
        } else if (gamepadCount === 1) {
            return [2, 3];
        }
        return [3, 4];
    }

    if (gamepadCount === 0) {
        return [1, 2]; // Keyboard controls both controllers
    } else if (gamepadCount === 1) {
        return [2]; // Keyboard controls controller 2
    } else {
        return []; // Two or more gamepads, keyboard disabled
    }
}

/**
 * Check if joypad input should be suppressed for a given controller.
 * 
 * Joypad input is suppressed on a port if that port has a mouse-emulated controller.
 * 
 * @param {Object} nes - NES emulator instance
 * @param {number} controller - Controller number (1 or 2)
 * @returns {boolean} True if joypad input should be suppressed
 */
export function shouldSuppressJoypadInput(nes, controller) {
    return nes.is_mouse_emulated_controller(controller);
}

