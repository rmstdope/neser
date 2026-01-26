export function mapStandardGamepadState(gamepad, axisThreshold = 0.5) {
    const buttons = gamepad?.buttons ?? [];
    const axes = gamepad?.axes ?? [];

    const up = Boolean(buttons[12]?.pressed) || axes[1] < -axisThreshold;
    const down = Boolean(buttons[13]?.pressed) || axes[1] > axisThreshold;
    const left = Boolean(buttons[14]?.pressed) || axes[0] < -axisThreshold;
    const right = Boolean(buttons[15]?.pressed) || axes[0] > axisThreshold;

    return {
        a: Boolean(buttons[0]?.pressed),
        b: Boolean(buttons[1]?.pressed),
        select: Boolean(buttons[8]?.pressed),
        start: Boolean(buttons[9]?.pressed),
        up,
        down,
        left,
        right
    };
}

export function selectPrimaryGamepad(gamepads) {
    if (!gamepads) return null;
    for (const gamepad of gamepads) {
        if (gamepad && gamepad.connected) {
            return gamepad;
        }
    }
    return null;
}
