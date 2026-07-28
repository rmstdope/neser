// Derive raw button layouts for browser-unmapped gamepads from the SDL
// gamecontrollerdb.txt that the repo already ships for the native frontend.
//
// Browsers expose unknown pads with `mapping: ""` and raw driver button
// order. The SDL db knows that order per pad and platform: its GUID encodes
// the USB vendor/product ids (which browsers also embed in `Gamepad.id`),
// and its a/b/x/y fields are positional (south/east/west/north).

/**
 * Raw indices into `gamepad.buttons` for the positional fields returned by
 * the gamepad mapper (a=south, b=east, y=west, x=north). `-1` marks a button
 * the pad does not have (or that is not a plain button in the SDL mapping).
 */
export interface RawButtonLayout {
    a: number;
    b: number;
    y: number;
    x: number;
    select: number;
    start: number;
    l: number;
    r: number;
}

/** Extract the USB vendor/product ids from an SDL GUID (little-endian). */
export function vendorProductFromGuid(guid: string): { vendor: string; product: string } | null {
    if (!/^[0-9a-fA-F]{32}$/.test(guid)) {
        return null;
    }
    const lower = guid.toLowerCase();
    const vendor = lower.slice(10, 12) + lower.slice(8, 10);
    const product = lower.slice(18, 20) + lower.slice(16, 18);
    return { vendor, product };
}

/**
 * Extract the USB vendor/product ids from a browser `Gamepad.id`.
 * Chrome: "USB gamepad (Vendor: 081f Product: e401)".
 * Firefox: "81f-e401-USB gamepad".
 */
export function extractVendorProduct(id: string): { vendor: string; product: string } | null {
    const pad = (hex: string) => hex.toLowerCase().padStart(4, "0");

    const chrome = /vendor:?\s*([0-9a-fA-F]{1,4})\s+product:?\s*([0-9a-fA-F]{1,4})/i.exec(id);
    if (chrome) {
        return { vendor: pad(chrome[1]), product: pad(chrome[2]) };
    }
    const firefox = /^([0-9a-fA-F]{1,4})-([0-9a-fA-F]{1,4})-/.exec(id);
    if (firefox) {
        return { vendor: pad(firefox[1]), product: pad(firefox[2]) };
    }
    return null;
}

/** Map a browser user agent to the SDL db platform field value. */
export function sdlPlatformForUserAgent(userAgent: string): string | null {
    if (/android/i.test(userAgent)) {
        return "Android";
    }
    if (/mac os x|macintosh/i.test(userAgent)) {
        return "Mac OS X";
    }
    if (/windows/i.test(userAgent)) {
        return "Windows";
    }
    if (/linux|cros/i.test(userAgent)) {
        return "Linux";
    }
    return null;
}

/** Parse "bN" SDL button values; anything else (axes, hats) yields -1. */
function buttonIndex(value: string | undefined): number {
    if (value === undefined) {
        return -1;
    }
    const match = /^b(\d+)$/.exec(value);
    return match ? Number(match[1]) : -1;
}

/**
 * Parse gamecontrollerdb.txt content into raw button layouts for one SDL
 * platform, keyed by "vendor:product". Later lines override earlier ones.
 * Lines whose south (a) or east (b) entries are not plain buttons are
 * skipped — without those two the layout is useless.
 */
export function parseGameControllerDb(
    text: string,
    platform: string
): Map<string, RawButtonLayout> {
    const layouts = new Map<string, RawButtonLayout>();

    for (const rawLine of text.split("\n")) {
        const line = rawLine.trim();
        if (line === "" || line.startsWith("#")) {
            continue;
        }
        const fields = line.split(",");
        const key = vendorProductFromGuid(fields[0]);
        if (!key) {
            continue;
        }

        const entries = new Map<string, string>();
        for (const field of fields.slice(2)) {
            const colon = field.indexOf(":");
            if (colon > 0) {
                entries.set(field.slice(0, colon), field.slice(colon + 1));
            }
        }
        if (entries.get("platform") !== platform) {
            continue;
        }

        const a = buttonIndex(entries.get("a"));
        const b = buttonIndex(entries.get("b"));
        if (a < 0 || b < 0) {
            continue;
        }

        layouts.set(`${key.vendor}:${key.product}`, {
            a,
            b,
            // SDL x is the west button and SDL y the north button.
            y: buttonIndex(entries.get("x")),
            x: buttonIndex(entries.get("y")),
            select: buttonIndex(entries.get("back")),
            start: buttonIndex(entries.get("start")),
            l: buttonIndex(entries.get("leftshoulder")),
            r: buttonIndex(entries.get("rightshoulder")),
        });
    }

    return layouts;
}
