// Default analog stick axis threshold used to interpret directional input.
// Callers can override this by passing a custom `axisThreshold` value.
const DEFAULT_AXIS_THRESHOLD = 0.5;

export function mapStandardGamepadState(gamepad: Gamepad | null, axisThreshold = DEFAULT_AXIS_THRESHOLD) {
    const buttons = gamepad?.buttons ?? [];
    const axes = gamepad?.axes ?? [];

    const up = Boolean(buttons[12]?.pressed) || axes[1] < -axisThreshold;
    const down = Boolean(buttons[13]?.pressed) || axes[1] > axisThreshold;
    const left = Boolean(buttons[14]?.pressed) || axes[0] < -axisThreshold;
    const right = Boolean(buttons[15]?.pressed) || axes[0] > axisThreshold;

    return {
        a: Boolean(buttons[0]?.pressed),
        b: Boolean(buttons[1]?.pressed),
        y: Boolean(buttons[2]?.pressed),
        x: Boolean(buttons[3]?.pressed),
        select: Boolean(buttons[8]?.pressed),
        start: Boolean(buttons[9]?.pressed),
        up,
        down,
        left,
        right,
        l: Boolean(buttons[4]?.pressed),
        r: Boolean(buttons[5]?.pressed)
    };
}

export function selectPrimaryGamepad(gamepads: (Gamepad | null)[]) {
    if (!gamepads) return null;
    for (const gamepad of gamepads) {
        if (gamepad && gamepad.connected) {
            return gamepad;
        }
    }
    return null;
}

/**
 * Select up to two connected gamepads from the gamepads array.
 * 
 * @param {Gamepad[]} gamepads - Array of gamepads (may contain null/undefined)
 * @returns {Gamepad[]} Array of connected gamepads (0-2 elements)
 */
export function selectGamepads(gamepads: (Gamepad | null)[]) {
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
