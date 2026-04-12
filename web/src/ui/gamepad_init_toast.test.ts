import { expect, it } from "vitest";
import { createGamepadInitToastNotifier } from "./gamepad_init_toast";

it("showOnce emits toast only on first invocation", () => {
    const shownMessages: string[] = [];
    let buildCount = 0;
    const notifier = createGamepadInitToastNotifier({
        buildMessage(enabled: boolean, count: number) {
            buildCount += 1;
            return enabled ? `enabled:${count}` : "disabled";
        },
        showToast(message: string) {
            shownMessages.push(message);
        }
    } as any);

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
        buildMessage(enabled: boolean, count: number) {
            received = { enabled, count };
            return "message";
        },
        showToast() {}
    } as any);

    notifier.showOnce(false, 0);

    expect(received).toEqual({ enabled: false, count: 0 });
});
