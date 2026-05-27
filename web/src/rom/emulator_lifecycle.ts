import type { WebRomConsoleKind } from "./rom_extensions";

export function shouldCreateFreshEmulatorForRomStart(
    currentKind: WebRomConsoleKind | null,
    nextKind: WebRomConsoleKind,
) {
    return currentKind !== nextKind || nextKind === "gba";
}
