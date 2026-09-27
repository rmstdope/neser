import { CONSOLES, type ConsoleKind } from "../console/consoles";

export function shouldCreateFreshEmulatorForRomStart(
    currentKind: ConsoleKind | null,
    nextKind: ConsoleKind,
) {
    return currentKind !== nextKind || CONSOLES[nextKind].freshInstanceOnStart;
}
