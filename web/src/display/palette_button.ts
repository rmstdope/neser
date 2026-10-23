import { CONSOLES, type ConsoleKind } from "../console/consoles";

/**
 * The Palette button does what F8 does, so it is shown only where F8 cycles
 * a palette: an NES game, or an original Game Boy game (the core then reports
 * a non-empty label), while a game runs. Paused, it keeps its place in the
 * control bar (so the bar does not shift) and the app disables it, because a
 * press must change the picture at once.
 */
export function paletteButtonVisible(state: {
    kind: ConsoleKind | null;
    label: string;
    running: boolean;
    paused: boolean;
}): boolean {
    return state.kind !== null && CONSOLES[state.kind].paletteButton && state.label !== "" && state.running;
}

/** A palette source that can only step to the next palette, as the cores can. */
export type PaletteCycler = { label(): string; cycle(): void };

/**
 * Every palette's name, from the current one onwards, by stepping once around; the source ends on
 * the palette it began on. Stops after `limit` steps if it never comes back round.
 */
export function listPalettes(source: PaletteCycler, limit = 32): string[] {
    const start = source.label();
    const names = [start];
    for (let i = 0; i < limit; i++) {
        source.cycle();
        const name = source.label();
        if (name === start) break;
        names.push(name);
    }
    return names;
}

/** Steps the source to the palette named `target`; false (back where it began) if it has none. */
export function cyclePaletteTo(source: PaletteCycler, target: string, limit = 32): boolean {
    const start = source.label();
    for (let i = 0; i < limit; i++) {
        if (source.label() === target) return true;
        source.cycle();
        if (source.label() === start) return false;
    }
    return false;
}

/**
 * The order the Palette list shows: the one first seen for this game, so entries stay in place
 * however the current palette moves; a listing with other names becomes the new order.
 */
export function stablePaletteOrder(firstSeen: string[] | null, listed: string[]): string[] {
    if (!firstSeen || firstSeen.length !== listed.length) return listed;
    const names = new Set(listed);
    return firstSeen.every((name) => names.has(name)) ? firstSeen : listed;
}
