/**
 * Which look the Filter button names and which one the picture shows.
 *
 * They differ only while a look's images are being fetched the first time: the button moves on
 * at once, the picture stays on the previous look until the new one is ready, and a look that
 * cannot be fetched puts the button back on the picture's look. A load that finishes after a
 * newer press, or after the selection was reset for another game, changes nothing.
 */

export interface FilterSelection {
    /** The look the picture is drawn with. */
    shown: string;
    /** The look the button names and the next press cycles from. */
    requested: string;
}

export function selectionAt(key: string): FilterSelection {
    return { shown: key, requested: key };
}

export function requestFilter(sel: FilterSelection, key: string): FilterSelection {
    return { shown: sel.shown, requested: key };
}

export function filterReady(sel: FilterSelection, key: string): FilterSelection {
    return sel.requested === key ? selectionAt(key) : sel;
}

export function filterFailed(sel: FilterSelection, key: string): FilterSelection {
    return sel.requested === key ? selectionAt(sel.shown) : sel;
}

export function buttonFilterKey(sel: FilterSelection): string {
    return sel.requested;
}
