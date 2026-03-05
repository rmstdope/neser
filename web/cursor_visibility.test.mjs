import test from "node:test";
import assert from "node:assert/strict";

import { computeMouseCursorStyle } from "./cursor_visibility.js";

function assertCursorStyle({ arkanoidActive, windowFocused, releasedByEscape }, expectedStyle) {
    assert.equal(
        computeMouseCursorStyle({ arkanoidActive, windowFocused, releasedByEscape }),
        expectedStyle
    );
}

test("computeMouseCursorStyle hides cursor while Arkanoid is active and window is focused", () => {
    assertCursorStyle({ arkanoidActive: true, windowFocused: true }, "none");
});

test("computeMouseCursorStyle restores cursor when window loses focus", () => {
    assertCursorStyle({ arkanoidActive: true, windowFocused: false }, "default");
});

test("computeMouseCursorStyle keeps cursor visible when Arkanoid is inactive", () => {
    assertCursorStyle({ arkanoidActive: false, windowFocused: true }, "default");
});

test("computeMouseCursorStyle restores cursor after Escape release override", () => {
    assertCursorStyle(
        {
            arkanoidActive: true,
            windowFocused: true,
            releasedByEscape: true,
        },
        "default"
    );
});
