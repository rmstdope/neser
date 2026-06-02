export const WEB_SHORTCUT_REFERENCE = [
    { key: "Space", action: "Pause/Resume" },
    { key: "Ctrl+R", action: "Soft Reset" },
    { key: "Shift+Ctrl+R", action: "Hard Reset" },
    { key: "F4", action: "Cycle Filter" },
    { key: "F5", action: "Debugger Toggle" },
    { key: "F6", action: "Save State" },
    { key: "F7", action: "Load State" },
    { key: "F8", action: "Cycle Palette (NES)" },
    { key: "F10", action: "Debugger Step Over" },
    { key: "F11", action: "Debugger Step Into" },
    { key: "Ctrl+F", action: "Toggle Fullscreen" },
    { key: "H", action: "Toggle Help" }
];

const PLAYER_KEYBOARD_BINDINGS = [
    "W/A/S/D: D-Pad\nR: A\nT: B\n4: Select\n5: Start",
    "I/J/K/L: D-Pad\nO: A\nP: B\n9: Select\n0: Start"
];

const AGB_KEYBOARD_BINDINGS = "W/A/S/D: D-Pad\nR: Y\nT: X\nF: B\nG: A\nV: L\nB: R\n4: Select\n5: Start";

export type HelpConsoleKind = "nes" | "gb" | "gba";

function buildPlayerSection(playerNumber: number, hasGamepad: boolean, keyBindings: string) {
    const controls = hasGamepad ? "Gamepad" : keyBindings;
    return `Controller (Player ${playerNumber})\n${controls}`;
}

export function buildControllerOverlayText(gamepadCount = 0, consoleKind: HelpConsoleKind = "nes") {
    if (consoleKind === "gb") {
        return buildPlayerSection(1, gamepadCount >= 1, PLAYER_KEYBOARD_BINDINGS[0]);
    }

    if (consoleKind === "gba") {
        return buildPlayerSection(1, gamepadCount >= 1, AGB_KEYBOARD_BINDINGS);
    }

    const player1 = buildPlayerSection(1, gamepadCount >= 1, PLAYER_KEYBOARD_BINDINGS[0]);
    const player2 = buildPlayerSection(2, gamepadCount >= 2, PLAYER_KEYBOARD_BINDINGS[1]);
    return `${player1}\n\n${player2}`;
}

export function buildFullHelpOverlayText(gamepadCount = 0, consoleKind: HelpConsoleKind = "nes") {
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
