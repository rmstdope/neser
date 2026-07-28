// Default analog stick axis threshold used to interpret directional input.
// Callers can override this by passing a custom `axisThreshold` value.
const DEFAULT_AXIS_THRESHOLD = 0.5;

/**
 * Raw button indices for the returned positional fields (a=south, b=east,
 * y=west, x=north), used for pads the browser exposes without a "standard"
 * mapping. Values index directly into `gamepad.buttons`.
 */
interface RawButtonLayout {
    a: number;
    b: number;
    y: number;
    x: number;
    select: number;
    start: number;
    l: number;
    r: number;
}

// The W3C standard gamepad layout.
const STANDARD_LAYOUT: RawButtonLayout = { a: 0, b: 1, y: 2, x: 3, select: 8, start: 9, l: 4, r: 5 };

// Generic SNES USB replica pads (vendor 081f, product e401) enumerate their
// buttons in raw HID order X, A, B, Y, ..., Select, Start. Browsers expose
// them without a standard mapping, so reading them positionally made
// physical X act as the south button.
const SNES_REPLICA_LAYOUT: RawButtonLayout = { a: 2, b: 1, y: 3, x: 0, select: 8, start: 9, l: 4, r: 5 };

/**
 * Pick the raw button layout for a pad: known non-standard pads are matched
 * by the vendor/product ids browsers embed in `Gamepad.id`; anything the
 * browser already standard-maps (or that we don't recognize) uses the
 * standard layout.
 */
function rawButtonLayoutFor(gamepad: Gamepad | null): RawButtonLayout {
    if (!gamepad || gamepad.mapping === "standard") {
        return STANDARD_LAYOUT;
    }
    const id = (gamepad.id ?? "").toLowerCase();
    if (id.includes("081f") && id.includes("e401")) {
        return SNES_REPLICA_LAYOUT;
    }
    return STANDARD_LAYOUT;
}

export function mapStandardGamepadState(gamepad: Gamepad | null, axisThreshold = DEFAULT_AXIS_THRESHOLD) {
    const buttons = gamepad?.buttons ?? [];
    const axes = gamepad?.axes ?? [];
    const layout = rawButtonLayoutFor(gamepad);

    const up = Boolean(buttons[12]?.pressed) || axes[1] < -axisThreshold;
    const down = Boolean(buttons[13]?.pressed) || axes[1] > axisThreshold;
    const left = Boolean(buttons[14]?.pressed) || axes[0] < -axisThreshold;
    const right = Boolean(buttons[15]?.pressed) || axes[0] > axisThreshold;

    return {
        a: Boolean(buttons[layout.a]?.pressed),
        b: Boolean(buttons[layout.b]?.pressed),
        y: Boolean(buttons[layout.y]?.pressed),
        x: Boolean(buttons[layout.x]?.pressed),
        select: Boolean(buttons[layout.select]?.pressed),
        start: Boolean(buttons[layout.start]?.pressed),
        up,
        down,
        left,
        right,
        l: Boolean(buttons[layout.l]?.pressed),
        r: Boolean(buttons[layout.r]?.pressed)
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
    return gamepads.filter((gamepad): gamepad is Gamepad => gamepad?.connected ?? false).slice(0, 2);
}
