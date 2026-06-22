import type { WebRomConsoleKind } from "../rom/rom_extensions";

export function supportsWebSaveState(kind: WebRomConsoleKind | null) {
    return kind === "nes" || kind === "snes";
}
