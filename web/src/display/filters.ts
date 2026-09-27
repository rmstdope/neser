/**
 * Pure filter-cycling logic, extracted for testability.
 *
 * The WebGL shader compilation / rendering stays in app.ts;
 * this module owns only the *selection* rules (which filters are
 * available for which console, how cycling works, and what happens
 * on a console switch).
 */

import { CONSOLES, type ConsoleKind } from "../console/consoles";

export interface FilterDef {
    name: string;
    /** "single" = 1-pass, "ntsc" = 2-pass NTSC, "gb" = 5-pass Game Boy, "gba" = a Game Boy Advance look (display/gba_pipeline.ts) */
    type: string;
    fragmentShader?: string;
    params?: Record<string, number>;
}

/** Return the ordered list of filter keys available for a given console. */
export function filterKeysForConsole(
    allFilterKeys: string[],
    filters: Record<string, FilterDef>,
    console: ConsoleKind,
): string[] {
    const family = CONSOLES[console].filterFamily;
    return allFilterKeys.filter((key) => {
        const f = filters[key];
        if (!f) return false;
        const isStock = f.type === "single" && key === "stock";
        // Game Boy family: stock + gb-type filters only
        if (family === "gb") return isStock || f.type === "gb";
        // Game Boy Advance family: stock + gba-type filters only
        if (family === "gba") return isStock || f.type === "gba";
        // NES family (NES and SNES): everything except the handheld looks
        return f.type !== "gb" && f.type !== "gba";
    });
}

/**
 * Cycle to the next filter for the given console.
 * Returns the new filter key.
 */
export function cycleFilterKey(
    currentFilter: string,
    allFilterKeys: string[],
    filters: Record<string, FilterDef>,
    console: ConsoleKind,
): string {
    const keys = filterKeysForConsole(allFilterKeys, filters, console);
    if (keys.length === 0) return currentFilter;
    const idx = keys.indexOf(currentFilter);
    return keys[(idx + 1) % keys.length];
}

/**
 * Determine the filter to use when switching consoles.
 * If the current filter is not available for the target console,
 * falls back to the console-appropriate default ("gameboy" for GB,
 * "ntsc" for NES) via {@link defaultFilterForConsole}.
 *
 * `untouched` is true while nobody has chosen a look and no game has loaded on
 * this page: the page's initial filter is then not a choice to carry over, so
 * the target console starts on its own default (None for SNES).
 */
export function filterOnConsoleSwitch(
    currentFilter: string,
    allFilterKeys: string[],
    filters: Record<string, FilterDef>,
    targetConsole: ConsoleKind,
    untouched = false,
): string {
    if (untouched) return defaultFilterForConsole(targetConsole);
    const keys = filterKeysForConsole(allFilterKeys, filters, targetConsole);
    if (keys.includes(currentFilter)) return currentFilter;
    return defaultFilterForConsole(targetConsole);
}

/** Return the preferred default filter key for a given console. */
export function defaultFilterForConsole(console: ConsoleKind): string {
    return CONSOLES[console].defaultFilter;
}

/**
 * Whether the WebGL filter pipeline must be rebuilt after a console switch.
 *
 * A changed filter always needs it. NTSC also needs it when the frame size
 * differs from the one its pass-1 target was built for (`width * 4` by
 * `height`): NTSC carries over between NES (240 wide) and SNES (256 wide)
 * games, and a stale target computes the composite pattern at the wrong
 * sample rate. Single-pass filters read the live frame size every frame.
 */
export function filterPipelineNeedsRebuild(
    previousFilter: string,
    nextFilter: string,
    filters: Record<string, FilterDef>,
    ntscTarget: { width: number; height: number },
    frame: { width: number; height: number },
): boolean {
    if (previousFilter !== nextFilter) return true;
    if (filters[nextFilter]?.type !== "ntsc") return false;
    return ntscTarget.width !== frame.width * 4 || ntscTarget.height !== frame.height;
}
