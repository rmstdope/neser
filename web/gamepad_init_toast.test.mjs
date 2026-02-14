import assert from "node:assert/strict";
import test from "node:test";
import { createGamepadInitToastNotifier } from "./gamepad_init_toast.js";

test("showOnce emits toast only on first invocation", () => {
    const shownMessages = [];
    let buildCount = 0;
    const notifier = createGamepadInitToastNotifier({
        buildMessage(enabled, count) {
            buildCount += 1;
            return enabled ? `enabled:${count}` : "disabled";
        },
        showToast(message) {
            shownMessages.push(message);
        }
    });

    const first = notifier.showOnce(true, 2);
    const second = notifier.showOnce(true, 1);

    assert.equal(first, true);
    assert.equal(second, false);
    assert.equal(buildCount, 1);
    assert.deepEqual(shownMessages, ["enabled:2"]);
});

test("showOnce uses provided message builder arguments", () => {
    let received = null;
    const notifier = createGamepadInitToastNotifier({
        buildMessage(enabled, count) {
            received = { enabled, count };
            return "message";
        },
        showToast() {}
    });

    notifier.showOnce(false, 0);

    assert.deepEqual(received, { enabled: false, count: 0 });
});
