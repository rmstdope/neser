import { CONSOLES, type ConsoleKind } from "../console/consoles";

export const WEB_SHORTCUT_REFERENCE = [
    { key: "Space", action: "Pause/Resume" },
    { key: "Ctrl+R", action: "Soft Reset" },
    { key: "Shift+Ctrl+R", action: "Hard Reset" },
    { key: "F4", action: "Cycle Filter" },
    { key: "F5", action: "Debugger Toggle" },
    { key: "F6", action: "Save State" },
    { key: "F7", action: "Load State" },
    { key: "F8", action: "Cycle Palette / Colors" },
    { key: "F10", action: "Debugger Step Over" },
    { key: "F11", action: "Debugger Step Into" },
    { key: "Ctrl+F", action: "Toggle Fullscreen" },
    { key: "H", action: "Toggle Help" }
];

function buildPlayerSection(playerNumber: number, hasGamepad: boolean, keyBindings: string) {
    const controls = hasGamepad ? "Gamepad" : keyBindings;
    return `Controller (Player ${playerNumber})\n${controls}`;
}

export function buildControllerOverlayText(gamepadCount = 0, consoleKind: ConsoleKind = "nes") {
    return CONSOLES[consoleKind].playerKeyBindings
        .map((keyBindings, index) => buildPlayerSection(index + 1, gamepadCount >= index + 1, keyBindings))
        .join("\n\n");
}

export function buildFullHelpOverlayText(gamepadCount = 0, consoleKind: ConsoleKind = "nes") {
    return buildShortcutOverlayText() + "\n\n" + buildControllerOverlayText(gamepadCount, consoleKind);
}

export function buildShortcutReferenceText(shortcuts = WEB_SHORTCUT_REFERENCE) {
    return shortcuts.map((shortcut) => `${shortcut.key} = ${shortcut.action}`).join(" | ");
}

export function buildShortcutOverlayText(shortcuts = WEB_SHORTCUT_REFERENCE) {
    const lines = shortcuts.map((shortcut) => `${shortcut.key}: ${shortcut.action}`);
    return ["Shortcuts", ...lines].join("\n");
}

export function computeShortcutHelpFontSizePx(canvasHeightPx: number) {
    const baselineHeight = 960;
    const baselineFontSize = 26;
    const scaled = Math.round((canvasHeightPx / baselineHeight) * baselineFontSize);
    return Math.max(12, Math.min(scaled, 38));
}

export function toggleShortcutHelpVisibility(helpOverlayElement: HTMLElement | null) {
    if (!helpOverlayElement) {
        return false;
    }

    const isHidden = helpOverlayElement.classList.contains("hidden");

    if (isHidden) {
        helpOverlayElement.classList.remove("hidden");
        helpOverlayElement.setAttribute("aria-hidden", "false");
        return true;
    }

    helpOverlayElement.classList.add("hidden");
    helpOverlayElement.setAttribute("aria-hidden", "true");
    return false;
}
