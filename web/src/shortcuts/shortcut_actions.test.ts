import { expect, it } from "vitest";

import { dispatchWebShortcutAction } from "./shortcut_actions";

function makeKeyboardEvent(code: string, modifiers: Record<string, boolean> = {}) {
    let prevented = false;
    return {
        code,
        repeat: false,
        metaKey: false,
        altKey: false,
        ctrlKey: false,
        shiftKey: false,
        ...modifiers,
        preventDefault() {
            prevented = true;
        },
        get defaultPrevented() {
            return prevented;
        }
    };
}

function makeActions() {
    const calls: string[] = [];
    return {
        calls,
        actions: {
            togglePause: () => calls.push("togglePause"),
            reset: () => calls.push("softReset"),
            hardReset: () => calls.push("hardReset"),
            toggleFilter: () => calls.push("toggleFilter"),
            saveState: async () => calls.push("saveState"),
            loadState: async () => calls.push("loadState"),
            toggleFullscreen: async () => calls.push("toggleFullscreen"),
            toggleHelp: () => calls.push("toggleHelp"),
            debuggerToggle: () => calls.push("debuggerToggle"),
            debuggerStepOver: () => calls.push("debuggerStepOver"),
            debuggerStepInto: () => calls.push("debuggerStepInto"),
            cyclePalette: () => calls.push("cyclePalette"),
        }
    };
}

it("dispatchWebShortcutAction routes Space to togglePause", async () => {
    const event = makeKeyboardEvent("Space");
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event as any, actions as any);

    expect(handled).toBe(true);
    expect(calls).toEqual(["togglePause"]);
    expect(event.defaultPrevented).toBe(true);
});

it("dispatchWebShortcutAction ignores legacy F1 reset key", async () => {
    const event = makeKeyboardEvent("F1");
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event as any, actions as any);

    expect(handled).toBe(false);
    expect(calls).toEqual([]);
    expect(event.defaultPrevented).toBe(false);
});

it("dispatchWebShortcutAction routes Ctrl+R to soft reset", async () => {
    const event = makeKeyboardEvent("KeyR", { ctrlKey: true });
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event as any, actions as any);

    expect(handled).toBe(true);
    expect(calls).toEqual(["softReset"]);
    expect(event.defaultPrevented).toBe(true);
});

it("dispatchWebShortcutAction ignores Ctrl+Alt+R (AltGr-like)", async () => {
    const event = makeKeyboardEvent("KeyR", { ctrlKey: true, altKey: true });
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event as any, actions as any);

    expect(handled).toBe(false);
    expect(calls).toEqual([]);
    expect(event.defaultPrevented).toBe(false);
});

it("dispatchWebShortcutAction ignores Alt+R", async () => {
    const event = makeKeyboardEvent("KeyR", { altKey: true });
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event as any, actions as any);

    expect(handled).toBe(false);
    expect(calls).toEqual([]);
    expect(event.defaultPrevented).toBe(false);
});

it("dispatchWebShortcutAction routes Shift+Ctrl+R to hard reset", async () => {
    const event = makeKeyboardEvent("KeyR", { ctrlKey: true, shiftKey: true });
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event as any, actions as any);

    expect(handled).toBe(true);
    expect(calls).toEqual(["hardReset"]);
    expect(event.defaultPrevented).toBe(true);
});

it("dispatchWebShortcutAction ignores Shift+Alt+R", async () => {
    const event = makeKeyboardEvent("KeyR", { altKey: true, shiftKey: true });
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event as any, actions as any);

    expect(handled).toBe(false);
    expect(calls).toEqual([]);
    expect(event.defaultPrevented).toBe(false);
});

it("dispatchWebShortcutAction routes F4 to toggleFilter", async () => {
    const event = makeKeyboardEvent("F4");
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event as any, actions as any);

    expect(handled).toBe(true);
    expect(calls).toEqual(["toggleFilter"]);
    expect(event.defaultPrevented).toBe(true);
});

it("dispatchWebShortcutAction routes F6 to saveState", async () => {
    const event = makeKeyboardEvent("F6");
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event as any, actions as any);

    expect(handled).toBe(true);
    expect(calls).toEqual(["saveState"]);
    expect(event.defaultPrevented).toBe(true);
});

it("dispatchWebShortcutAction routes F7 to loadState", async () => {
    const event = makeKeyboardEvent("F7");
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event as any, actions as any);

    expect(handled).toBe(true);
    expect(calls).toEqual(["loadState"]);
    expect(event.defaultPrevented).toBe(true);
});

it("dispatchWebShortcutAction ignores legacy F12 fullscreen key", async () => {
    const event = makeKeyboardEvent("F12");
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event as any, actions as any);

    expect(handled).toBe(false);
    expect(calls).toEqual([]);
    expect(event.defaultPrevented).toBe(false);
});

it("dispatchWebShortcutAction routes Ctrl+F to toggleFullscreen", async () => {
    const event = makeKeyboardEvent("KeyF", { ctrlKey: true });
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event as any, actions as any);

    expect(handled).toBe(true);
    expect(calls).toEqual(["toggleFullscreen"]);
    expect(event.defaultPrevented).toBe(true);
});

it("dispatchWebShortcutAction ignores Alt+F", async () => {
    const event = makeKeyboardEvent("KeyF", { altKey: true });
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event as any, actions as any);

    expect(handled).toBe(false);
    expect(calls).toEqual([]);
    expect(event.defaultPrevented).toBe(false);
});

it("dispatchWebShortcutAction routes H to toggleHelp", async () => {
    const event = makeKeyboardEvent("KeyH");
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event as any, actions as any);

    expect(handled).toBe(true);
    expect(calls).toEqual(["toggleHelp"]);
    expect(event.defaultPrevented).toBe(true);
});

it("dispatchWebShortcutAction ignores unknown keys", async () => {
    const event = makeKeyboardEvent("KeyQ");
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event as any, actions as any);

    expect(handled).toBe(false);
    expect(calls).toEqual([]);
    expect(event.defaultPrevented).toBe(false);
});

it("dispatchWebShortcutAction ignores repeated key events", async () => {
    const event = makeKeyboardEvent("Space");
    event.repeat = true;
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event as any, actions as any);

    expect(handled).toBe(false);
    expect(calls).toEqual([]);
    expect(event.defaultPrevented).toBe(false);
});

it("dispatchWebShortcutAction routes F5 to debuggerToggle", async () => {
    const event = makeKeyboardEvent("F5");
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event as any, actions as any);

    expect(handled).toBe(true);
    expect(calls).toEqual(["debuggerToggle"]);
    expect(event.defaultPrevented).toBe(true);
});

it("dispatchWebShortcutAction routes F10 to debuggerStepOver", async () => {
    const event = makeKeyboardEvent("F10");
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event as any, actions as any);

    expect(handled).toBe(true);
    expect(calls).toEqual(["debuggerStepOver"]);
    expect(event.defaultPrevented).toBe(true);
});

it("dispatchWebShortcutAction routes F11 to debuggerStepInto", async () => {
    const event = makeKeyboardEvent("F11");
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event as any, actions as any);

    expect(handled).toBe(true);
    expect(calls).toEqual(["debuggerStepInto"]);
    expect(event.defaultPrevented).toBe(true);
});

it("dispatchWebShortcutAction routes F8 to cyclePalette", async () => {
    const event = makeKeyboardEvent("F8");
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event as any, actions as any);

    expect(handled).toBe(true);
    expect(calls).toEqual(["cyclePalette"]);
    expect(event.defaultPrevented).toBe(true);
});
