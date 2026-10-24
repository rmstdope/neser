/** A field the player is typing into: keys go to it, not to the game or a shortcut. */
export function isEditableKeyboardTarget(target: EventTarget | null) {
    if (!(target instanceof HTMLElement)) {
        return false;
    }
    const tag = target.tagName.toUpperCase();
    return target.isContentEditable || tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
}

/** Inside an open dialog (the settings panel, autorun, firmware): its keys are the dialog's. */
export function belongsToOpenDialog(target: EventTarget | null) {
    return target instanceof Element && target.closest("dialog[open]") !== null;
}
