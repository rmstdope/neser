import { expect, it } from "vitest";

import {
    shouldForwardArkanoidMouseInput,
    shouldKeepPointerLocked,
} from "./pointer_lock";

it("shouldKeepPointerLocked returns true while Arkanoid is active and tab focused", () => {
    expect(
        shouldKeepPointerLocked({
            arkanoidActive: true,
            windowFocused: true,
            releasedByEscape: false,
        })
    ).toBe(true);
});

it("shouldKeepPointerLocked returns false when browser tab loses focus", () => {
    expect(
        shouldKeepPointerLocked({
            arkanoidActive: true,
            windowFocused: false,
            releasedByEscape: false,
        })
    ).toBe(false);
});

it("shouldKeepPointerLocked returns false after user presses Escape", () => {
    expect(
        shouldKeepPointerLocked({
            arkanoidActive: true,
            windowFocused: true,
            releasedByEscape: true,
        })
    ).toBe(false);
});

it("shouldForwardArkanoidMouseInput returns true only while pointer is grabbed", () => {
    expect(
        shouldForwardArkanoidMouseInput({ pointerLocked: true })
    ).toBe(true);
    expect(
        shouldForwardArkanoidMouseInput({ pointerLocked: false })
    ).toBe(false);
});
