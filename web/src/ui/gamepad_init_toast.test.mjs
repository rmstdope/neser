import { expect, it } from "vitest";
import { createGamepadInitToastNotifier } from "./gamepad_init_toast.js";

it("showOnce emits toast only on first invocation", () => {
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

    expect(first).toBe(true);
    expect(second).toBe(false);
    expect(buildCount).toBe(1);
    expect(shownMessages).toEqual(["enabled:2"]);
});

it("showOnce uses provided message builder arguments", () => {
    let received = null;
    const notifier = createGamepadInitToastNotifier({
        buildMessage(enabled, count) {
            received = { enabled, count };
            return "message";
        },
        showToast() {}
    });

    notifier.showOnce(false, 0);

    expect(received).toEqual({ enabled: false, count: 0 });
});
