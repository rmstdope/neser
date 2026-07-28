import gamecontrollerDbUrl from "../../../gamecontrollerdb.txt?url";

import {
    extractVendorProduct,
    parseGameControllerDb,
    sdlPlatformForUserAgent,
    type RawButtonLayout,
} from "./sdl_mapping";

// Default analog stick axis threshold used to interpret directional input.
// Callers can override this by passing a custom `axisThreshold` value.
const DEFAULT_AXIS_THRESHOLD = 0.5;

// The W3C standard gamepad layout.
const STANDARD_LAYOUT: RawButtonLayout = { a: 0, b: 1, y: 2, x: 3, select: 8, start: 9, l: 4, r: 5 };

/**
 * Raw layouts for pads the browser exposes without a "standard" mapping,
 * keyed by "vendor:product". Populated from the bundled
 * gamecontrollerdb.txt, which is the single source of truth for pad
 * layouts (shared with the native frontend).
 */
const rawLayoutRegistry = new Map<string, RawButtonLayout>();

/**
 * Load raw button layouts for the current platform from the bundled SDL
 * gamecontrollerdb.txt (the same file the native frontend uses). On
 * failure, unmapped pads fall back to the standard interpretation.
 */
export async function loadRawButtonLayoutsFromDb(
    fetchFn: typeof fetch = fetch,
    userAgent: string = typeof navigator !== "undefined" ? navigator.userAgent : ""
): Promise<void> {
    const platform = sdlPlatformForUserAgent(userAgent);
    if (!platform) {
        return;
    }
    try {
        const response = await fetchFn(gamecontrollerDbUrl);
        if (!response.ok) {
            return;
        }
        const layouts = parseGameControllerDb(await response.text(), platform);
        for (const [key, layout] of layouts) {
            rawLayoutRegistry.set(key, layout);
        }
    } catch {
        // Unmapped pads fall back to the standard interpretation until the
        // db can be loaded.
    }
}

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
    const key = extractVendorProduct(gamepad.id ?? "");
    if (key) {
        const layout = rawLayoutRegistry.get(`${key.vendor}:${key.product}`);
        if (layout) {
            return layout;
        }
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
