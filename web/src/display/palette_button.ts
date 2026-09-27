import type { WebRomConsoleKind } from "../rom/rom_extensions";

/**
 * The Palette button does what F8 does, so it is shown only where F8 cycles
 * a palette: an NES game, or an original Game Boy game (the core then reports
 * a non-empty label). Like the Colors button it is hidden while paused or
 * stopped, because a press must change the picture at once.
 */
export function paletteButtonVisible(state: {
    kind: WebRomConsoleKind | null;
    label: string;
    running: boolean;
    paused: boolean;
}): boolean {
    return (state.kind === "nes" || state.kind === "gb") && state.label !== "" && state.running && !state.paused;
}
