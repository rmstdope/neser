import type { WebRomConsoleKind } from "./rom_extensions";

/**
 * "Game Boy games run on" (nr-zdy.4): the console original Game Boy games
 * run on, "dmg" (Game Boy, the first-visit default) or "cgb" (Game Boy
 * Color). Game Boy Color games always run on the Game Boy Color.
 */
export type GbHardwareChoice = "dmg" | "cgb";

export const GB_HARDWARE_STORAGE_KEY = "neser.gbHardware";

/** This browser's `localStorage`, or `null` where the page may not use it. */
export function browserStorage(): Storage | null {
    try {
        return globalThis.localStorage ?? null;
    } catch {
        return null;
    }
}

/** The remembered choice; "dmg" on a first visit or when nothing can be read. */
export function readGbHardwareChoice(storage: Pick<Storage, "getItem"> | null): GbHardwareChoice {
    try {
        return storage?.getItem(GB_HARDWARE_STORAGE_KEY) === "cgb" ? "cgb" : "dmg";
    } catch {
        return "dmg";
    }
}

/** Remembers the choice in this browser, silently doing nothing where it cannot. */
export function writeGbHardwareChoice(storage: Pick<Storage, "setItem"> | null, choice: GbHardwareChoice): void {
    try {
        storage?.setItem(GB_HARDWARE_STORAGE_KEY, choice);
    } catch {
        // The choice then lasts for this page only.
    }
}

/**
 * "Applies when you press Reset" shows only while an original Game Boy game
 * runs on a different console from the one chosen.
 */
export function gbHardwareNoteVisible(state: {
    kind: WebRomConsoleKind | null;
    running: boolean;
    isOriginalGame: boolean;
    runsOnColor: boolean;
    choice: GbHardwareChoice;
}): boolean {
    return state.kind === "gb" && state.running && state.isOriginalGame && state.runsOnColor !== (state.choice === "cgb");
}
