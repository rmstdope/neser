import { CONSOLES, type ConsoleKind } from "../console/consoles";

export function supportsWebSaveState(kind: ConsoleKind | null) {
    return kind !== null && CONSOLES[kind].saveState;
}
