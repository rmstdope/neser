import { expect, it } from "vitest";

import { computeMouseCursorStyle } from "./cursor_visibility.js";

function assertCursorStyle({ arkanoidActive, windowFocused, releasedByEscape }, expectedStyle) {
    expect(
        computeMouseCursorStyle({ arkanoidActive, windowFocused, releasedByEscape })
    ).toBe(expectedStyle);
}

it("computeMouseCursorStyle hides cursor while Arkanoid is active and window is focused", () => {
    assertCursorStyle({ arkanoidActive: true, windowFocused: true }, "none");
});

it("computeMouseCursorStyle restores cursor when window loses focus", () => {
    assertCursorStyle({ arkanoidActive: true, windowFocused: false }, "default");
});

it("computeMouseCursorStyle keeps cursor visible when Arkanoid is inactive", () => {
    assertCursorStyle({ arkanoidActive: false, windowFocused: true }, "default");
});

it("computeMouseCursorStyle restores cursor after Escape release override", () => {
    assertCursorStyle(
        {
            arkanoidActive: true,
            windowFocused: true,
            releasedByEscape: true,
        },
        "default"
    );
});
