import type { WebRomConsoleKind } from "../rom/rom_extensions";

/** Label of the top-bar button: it names the current colour state. */
export function cgbColorButtonLabel(enabled: boolean): string {
    return enabled ? "Colors: GBC screen" : "Colors: Raw";
}

/**
 * The Colors button is shown only while a game is running whose colours can
 * be corrected: on the Game Boy core, Game Boy Color games and black-and-white
 * games the Game Boy Color colourises; and every Game Boy Advance game. It is
 * hidden while paused, because no new frame would be drawn and a press must
 * change the picture at once.
 */
export function cgbColorButtonVisible(state: {
    kind: WebRomConsoleKind | null;
    isColor: boolean;
    running: boolean;
    paused: boolean;
}): boolean {
    const correctable = (state.kind === "gb" && state.isColor) || state.kind === "gba";
    return correctable && state.running && !state.paused;
}
