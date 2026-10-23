/** Where the "Show debugger button" switch is remembered between visits. */
export const DEBUGGER_BUTTON_STORAGE_KEY = "neser.debuggerButton";

/** Whether the top bar shows the debugger's bug button; off on a first visit or without storage. */
export function readDebuggerButton(storage: Pick<Storage, "getItem"> | null): boolean {
    try {
        return storage?.getItem(DEBUGGER_BUTTON_STORAGE_KEY) === "on";
    } catch {
        return false;
    }
}

/** Remembers the switch; a browser that refuses storage keeps it for this visit only. */
export function writeDebuggerButton(storage: Pick<Storage, "setItem"> | null, on: boolean): void {
    try {
        storage?.setItem(DEBUGGER_BUTTON_STORAGE_KEY, on ? "on" : "off");
    } catch {
        // Private windows may refuse storage.
    }
}
