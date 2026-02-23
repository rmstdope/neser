import test from "node:test";
import assert from "node:assert/strict";

import { dispatchWebShortcutAction } from "./shortcut_actions.js";

function makeKeyboardEvent(code) {
    let prevented = false;
    return {
        code,
        repeat: false,
        preventDefault() {
            prevented = true;
        },
        get defaultPrevented() {
            return prevented;
        }
    };
}

function makeActions() {
    const calls = [];
    return {
        calls,
        actions: {
            togglePause: () => calls.push("togglePause"),
            reset: () => calls.push("reset"),
            toggleFilter: () => calls.push("toggleFilter"),
            saveState: async () => calls.push("saveState"),
            loadState: async () => calls.push("loadState"),
            toggleFullscreen: async () => calls.push("toggleFullscreen"),
            toggleHelp: () => calls.push("toggleHelp"),
            debuggerToggle: () => calls.push("debuggerToggle"),
            debuggerStepOver: () => calls.push("debuggerStepOver"),
            debuggerStepInto: () => calls.push("debuggerStepInto"),
        }
    };
}

test("dispatchWebShortcutAction routes Space to togglePause", async () => {
    const event = makeKeyboardEvent("Space");
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event, actions);

    assert.equal(handled, true);
    assert.deepEqual(calls, ["togglePause"]);
    assert.equal(event.defaultPrevented, true);
});

test("dispatchWebShortcutAction routes F1 to reset", async () => {
    const event = makeKeyboardEvent("F1");
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event, actions);

    assert.equal(handled, true);
    assert.deepEqual(calls, ["reset"]);
    assert.equal(event.defaultPrevented, true);
});

test("dispatchWebShortcutAction routes F4 to toggleFilter", async () => {
    const event = makeKeyboardEvent("F4");
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event, actions);

    assert.equal(handled, true);
    assert.deepEqual(calls, ["toggleFilter"]);
    assert.equal(event.defaultPrevented, true);
});

test("dispatchWebShortcutAction routes F6 to saveState", async () => {
    const event = makeKeyboardEvent("F6");
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event, actions);

    assert.equal(handled, true);
    assert.deepEqual(calls, ["saveState"]);
    assert.equal(event.defaultPrevented, true);
});

test("dispatchWebShortcutAction routes F7 to loadState", async () => {
    const event = makeKeyboardEvent("F7");
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event, actions);

    assert.equal(handled, true);
    assert.deepEqual(calls, ["loadState"]);
    assert.equal(event.defaultPrevented, true);
});

test("dispatchWebShortcutAction routes F12 to toggleFullscreen", async () => {
    const event = makeKeyboardEvent("F12");
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event, actions);

    assert.equal(handled, true);
    assert.deepEqual(calls, ["toggleFullscreen"]);
    assert.equal(event.defaultPrevented, true);
});

test("dispatchWebShortcutAction routes H to toggleHelp", async () => {
    const event = makeKeyboardEvent("KeyH");
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event, actions);

    assert.equal(handled, true);
    assert.deepEqual(calls, ["toggleHelp"]);
    assert.equal(event.defaultPrevented, true);
});

test("dispatchWebShortcutAction ignores unknown keys", async () => {
    const event = makeKeyboardEvent("KeyQ");
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event, actions);

    assert.equal(handled, false);
    assert.deepEqual(calls, []);
    assert.equal(event.defaultPrevented, false);
});

test("dispatchWebShortcutAction ignores repeated key events", async () => {
    const event = makeKeyboardEvent("Space");
    event.repeat = true;
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event, actions);

    assert.equal(handled, false);
    assert.deepEqual(calls, []);
    assert.equal(event.defaultPrevented, false);
});

test("dispatchWebShortcutAction routes F5 to debuggerToggle", async () => {
    const event = makeKeyboardEvent("F5");
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event, actions);

    assert.equal(handled, true);
    assert.deepEqual(calls, ["debuggerToggle"]);
    assert.equal(event.defaultPrevented, true);
});

test("dispatchWebShortcutAction routes F10 to debuggerStepOver", async () => {
    const event = makeKeyboardEvent("F10");
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event, actions);

    assert.equal(handled, true);
    assert.deepEqual(calls, ["debuggerStepOver"]);
    assert.equal(event.defaultPrevented, true);
});

test("dispatchWebShortcutAction routes F11 to debuggerStepInto", async () => {
    const event = makeKeyboardEvent("F11");
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event, actions);

    assert.equal(handled, true);
    assert.deepEqual(calls, ["debuggerStepInto"]);
    assert.equal(event.defaultPrevented, true);
});