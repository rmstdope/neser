import test from "node:test";
import assert from "node:assert/strict";

import { dispatchWebShortcutAction } from "./shortcut_actions.js";

function makeKeyboardEvent(code) {
    let prevented = false;
    return {
        code,
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
            saveState: async () => calls.push("saveState"),
            loadState: async () => calls.push("loadState"),
            toggleFullscreen: async () => calls.push("toggleFullscreen")
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

test("dispatchWebShortcutAction ignores unknown keys", async () => {
    const event = makeKeyboardEvent("KeyQ");
    const { calls, actions } = makeActions();

    const handled = await dispatchWebShortcutAction(event, actions);

    assert.equal(handled, false);
    assert.deepEqual(calls, []);
    assert.equal(event.defaultPrevented, false);
});