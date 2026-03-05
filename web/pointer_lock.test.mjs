import test from "node:test";
import assert from "node:assert/strict";

import {
    shouldForwardArkanoidMouseInput,
    shouldKeepPointerLocked,
} from "./pointer_lock.js";

test("shouldKeepPointerLocked returns true while Arkanoid is active and tab focused", () => {
    assert.equal(
        shouldKeepPointerLocked({
            arkanoidActive: true,
            windowFocused: true,
            releasedByEscape: false,
        }),
        true
    );
});

test("shouldKeepPointerLocked returns false when browser tab loses focus", () => {
    assert.equal(
        shouldKeepPointerLocked({
            arkanoidActive: true,
            windowFocused: false,
            releasedByEscape: false,
        }),
        false
    );
});

test("shouldKeepPointerLocked returns false after user presses Escape", () => {
    assert.equal(
        shouldKeepPointerLocked({
            arkanoidActive: true,
            windowFocused: true,
            releasedByEscape: true,
        }),
        false
    );
});

test("shouldForwardArkanoidMouseInput returns true only while pointer is grabbed", () => {
    assert.equal(
        shouldForwardArkanoidMouseInput({ pointerLocked: true }),
        true
    );
    assert.equal(
        shouldForwardArkanoidMouseInput({ pointerLocked: false }),
        false
    );
});
