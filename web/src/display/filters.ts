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
    /** "single" = 1-pass, "ntsc" = 2-pass NTSC, "gb" = 5-pass Game Boy */
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
        if (family === "stock") return isStock;
        // Game Boy family: stock + gb-type filters only
        if (family === "gb") return isStock || f.type === "gb";
        // NES family (NES and SNES): everything except gb-type filters
        return f.type !== "gb";
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
