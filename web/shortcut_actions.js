/**
 * Dispatches a keyboard event to a supported web shortcut action.
 *
 * Returns true when a mapped shortcut was handled.
 *
 * @param {KeyboardEvent|{code:string,preventDefault:Function}} event
 * @param {{
 *   togglePause: Function,
 *   reset: Function,
 *   saveState: Function,
 *   loadState: Function,
 *   toggleFullscreen: Function
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
    F6: "saveState",
    F7: "loadState",
    F12: "toggleFullscreen"
};

function shortcutActionForCode(code, actions) {
    const actionName = shortcutActionByCode[code];
    if (!actionName) {
        return null;
    }
    return actions[actionName];
}
