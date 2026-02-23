/**
 * Dispatches a keyboard event to a supported web shortcut action.
 *
 * Returns true when a mapped shortcut was handled.
 *
 * @param {KeyboardEvent|{code:string,preventDefault:Function}} event
 * @param {{
 *   togglePause: Function,
 *   reset: Function,
 *   toggleFilter: Function,
 *   saveState: Function,
 *   loadState: Function,
 *   toggleFullscreen: Function,
 *   toggleHelp: Function,
 *   debuggerToggle: Function,
 *   debuggerStepOver: Function,
 *   debuggerStepInto: Function
 * }} actions
 * @returns {Promise<boolean>}
 */
export async function dispatchWebShortcutAction(event, actions) {
    if (event.repeat) {
        return false;
    }

    const action = shortcutActionForCode(event.code, actions);
    if (!action) {
        return false;
    }

    event.preventDefault();
    await action();
    return true;
}

const shortcutActionByCode = {
    Space: "togglePause",
    F1: "reset",
    F4: "toggleFilter",
    F5: "debuggerToggle",
    F6: "saveState",
    F7: "loadState",
    F10: "debuggerStepOver",
    F11: "debuggerStepInto",
    F12: "toggleFullscreen",
    KeyH: "toggleHelp"
};

function shortcutActionForCode(code, actions) {
    const actionName = shortcutActionByCode[code];
    if (!actionName) {
        return null;
    }
    return actions[actionName];
}
