import { CONSOLES, type ConsoleKind } from "../console/consoles";
import { keyBindingRows } from "./settings_panel";

/** The key text for one player of a console, as its row in the console table gives it. */
export function helpPadKeys(kind: ConsoleKind, player: number): string {
    const players = CONSOLES[kind].playerKeyBindings;
    return players[Math.min(player, players.length - 1)];
}

/** The shortcuts the keys and controls panel lists, in its words, with their keys as caps. */
export const HELP_SHORTCUTS: readonly { keys: string[]; action: string }[] = [
    { keys: ["Space"], action: "Pause / resume" },
    { keys: ["Ctrl", "R"], action: "Soft reset" },
    { keys: ["⇧", "Ctrl", "R"], action: "Hard reset" },
    { keys: ["F6", "F7"], action: "Save / load state" },
    { keys: ["F4"], action: "Next filter" },
    { keys: ["F8"], action: "Next palette / colours" },
    { keys: ["Ctrl", "F"], action: "Fullscreen" },
    { keys: ["F5"], action: "Debugger" },
    { keys: ["F10", "F11"], action: "Step over / into" },
    { keys: ["H"], action: "These keys and controls" },
];

function cap(key: string): HTMLElement {
    const kbd = document.createElement("kbd");
    kbd.className = "kbd kbd-sm";
    kbd.textContent = key;
    return kbd;
}

/** Draws one row per shortcut into `list`: its key caps, then what it does. */
export function renderHelpShortcuts(list: HTMLElement): void {
    list.replaceChildren(
        ...HELP_SHORTCUTS.map(({ keys, action }) => {
            const row = document.createElement("div");
            row.className = "help-shortcut";
            const caps = document.createElement("span");
            caps.className = "help-shortcut-keys";
            caps.append(...keys.map(cap));
            const what = document.createElement("span");
            what.className = "help-shortcut-action";
            what.textContent = action;
            row.append(caps, what);
            return row;
        })
    );
}

/**
 * Draws a controller into `pad` for one player's key text: a D-pad, Select and Start, the face
 * buttons (two, or four where the console has X and Y) and the shoulders where it has them, each
 * with the keys that press it. Buttons the console lacks are left out.
 */
export function renderPad(pad: HTMLElement, keyText: string): void {
    const rows = keyBindingRows(keyText);
    const buttons = new Map(rows.map((row) => [row.action, row.keys]));
    pad.dataset.layout = buttons.has("X") ? "four-button" : "two-button";
    pad.replaceChildren(
        ...rows.map(({ action, keys }) => {
            const button = document.createElement("div");
            button.className = "pad-button";
            button.dataset.button = action;
            const shape = document.createElement("span");
            shape.className = "pad-shape";
            shape.setAttribute("aria-hidden", "true");
            if (action.length === 1) shape.textContent = action;
            const name = document.createElement("span");
            name.className = "pad-name";
            name.textContent = action.length === 1 ? "" : action;
            const caps = document.createElement("span");
            caps.className = "pad-keys";
            caps.append(...keys.map(cap));
            button.setAttribute("aria-label", `${action}: ${keys.join(", ")}`);
            button.append(shape, name, caps);
            return button;
        })
    );
}
