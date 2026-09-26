import type { WebRomConsoleKind } from "../rom/rom_extensions";

/** Label of the top-bar button: it names the current colour state. */
export function cgbColorButtonLabel(enabled: boolean): string {
    return enabled ? "Colors: GBC screen" : "Colors: Raw";
}

/**
 * The button is shown only while a game is running (or paused) in colour on
 * the Game Boy core: Game Boy Color games and black-and-white games the Game
 * Boy Color colourises.
 */
export function cgbColorButtonVisible(state: {
    kind: WebRomConsoleKind | null;
    isColor: boolean;
    active: boolean;
}): boolean {
    return state.kind === "gb" && state.isColor && state.active;
}
