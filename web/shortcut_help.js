export const WEB_SHORTCUT_REFERENCE = [
    { key: "Space", action: "Pause/Resume" },
    { key: "F1", action: "Reset" },
    { key: "F6", action: "Save State" },
    { key: "F7", action: "Load State" },
    { key: "F12", action: "Fullscreen" },
    { key: "H", action: "Toggle Help" }
];

const PLAYER_KEYBOARD_BINDINGS = [
    "W/A/S/D: D-Pad\nR: A\nT: B\n4: Select\n5: Start",
    "I/J/K/L: D-Pad\nO: A\nP: B\n9: Select\n0: Start"
];

function buildPlayerSection(playerNumber, hasGamepad, keyBindings) {
    const controls = hasGamepad ? "Gamepad" : keyBindings;
    return `Controller (Player ${playerNumber})\n${controls}`;
}

export function buildControllerOverlayText(gamepadCount = 0) {
    const player1 = buildPlayerSection(1, gamepadCount >= 1, PLAYER_KEYBOARD_BINDINGS[0]);
    const player2 = buildPlayerSection(2, gamepadCount >= 2, PLAYER_KEYBOARD_BINDINGS[1]);
    return `${player1}\n\n${player2}`;
}

export function buildFullHelpOverlayText(gamepadCount = 0) {
    return buildShortcutOverlayText() + "\n\n" + buildControllerOverlayText(gamepadCount);
}

export function buildShortcutReferenceText(shortcuts = WEB_SHORTCUT_REFERENCE) {
    return shortcuts.map((shortcut) => `${shortcut.key} = ${shortcut.action}`).join(" | ");
}

export function buildShortcutOverlayText(shortcuts = WEB_SHORTCUT_REFERENCE) {
    const lines = shortcuts.map((shortcut) => `${shortcut.key}: ${shortcut.action}`);
    return ["Shortcuts", ...lines].join("\n");
}

export function computeShortcutHelpFontSizePx(canvasHeightPx) {
    const baselineHeight = 960;
    const baselineFontSize = 26;
    const scaled = Math.round((canvasHeightPx / baselineHeight) * baselineFontSize);
    return Math.max(12, Math.min(scaled, 38));
}

export function toggleShortcutHelpVisibility(helpOverlayElement) {
    if (!helpOverlayElement) {
        return false;
    }

    const isHidden = helpOverlayElement.classList.contains("d-none");

    if (isHidden) {
        helpOverlayElement.classList.remove("d-none");
        helpOverlayElement.setAttribute("aria-hidden", "false");
        return true;
    }

    helpOverlayElement.classList.add("d-none");
    helpOverlayElement.setAttribute("aria-hidden", "true");
    return false;
}
