/**
 * Dispatches a keyboard event to a supported web shortcut action.
 *
 * Returns true when a mapped shortcut was handled.
 *
 * @param {KeyboardEvent|{code:string,repeat:boolean,metaKey:boolean,altKey:boolean,ctrlKey:boolean,shiftKey:boolean,preventDefault:Function}} event
 * @param {{
 *   togglePause: Function,
 *   reset: Function,
 *   hardReset: Function,
 *   toggleFilter: Function,
 *   saveState: Function,
 *   loadState: Function,
 *   toggleFullscreen: Function,
 *   toggleHelp: Function,
 *   debuggerToggle: Function,
 *   debuggerStepOver: Function,
 *   debuggerStepInto: Function,
 *   cyclePalette: Function
 * }} actions
 * @returns {Promise<boolean>}
 */
interface ShortcutEvent {
    code: string;
    repeat: boolean;
    metaKey: boolean;
    altKey: boolean;
    ctrlKey: boolean;
    shiftKey: boolean;
    preventDefault: () => void;
}

interface ShortcutActions {
    togglePause: () => Promise<void> | void;
    reset: () => Promise<void> | void;
    hardReset: () => Promise<void> | void;
    toggleFilter: () => Promise<void> | void;
    saveState: () => Promise<void> | void;
    loadState: () => Promise<void> | void;
    toggleFullscreen: () => Promise<void> | void;
    toggleHelp: () => Promise<void> | void;
    debuggerToggle: () => Promise<void> | void;
    debuggerStepOver: () => Promise<void> | void;
    debuggerStepInto: () => Promise<void> | void;
    cyclePalette: () => Promise<void> | void;
    [key: string]: (() => Promise<void> | void) | undefined;
}

export async function dispatchWebShortcutAction(event: ShortcutEvent, actions: ShortcutActions) {
    if (event.repeat) {
        return false;
    }

    const action = shortcutActionForEvent(event, actions);
    if (!action) {
        return false;
    }

    event.preventDefault();
    await action();
    return true;
}

const shortcutActionByCode = {
    Space: "togglePause",
    F4: "toggleFilter",
    F5: "debuggerToggle",
    F6: "saveState",
    F7: "loadState",
    F8: "cyclePalette",
    F10: "debuggerStepOver",
    F11: "debuggerStepInto",
    KeyH: "toggleHelp"
};

function hasControlModifier(event: ShortcutEvent) {
    return Boolean(event.ctrlKey) && !event.altKey;
}

function shortcutActionForEvent(event: ShortcutEvent, actions: ShortcutActions) {
    if (hasControlModifier(event)) {
        if (event.code === "KeyR") {
            return event.shiftKey ? actions.hardReset : actions.reset;
        }

        if (event.code === "KeyF") {
            return actions.toggleFullscreen;
        }
    }

    const code = event.code;
    const actionName = shortcutActionByCode[code as keyof typeof shortcutActionByCode];
    if (!actionName) {
        return null;
    }
    return actions[actionName];
}
