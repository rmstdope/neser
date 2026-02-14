import assert from "node:assert/strict";
import test from "node:test";

import {
    WEB_SHORTCUT_REFERENCE,
    buildShortcutOverlayText,
    buildShortcutReferenceText,
    computeShortcutHelpFontSizePx,
    toggleShortcutHelpVisibility
} from "./shortcut_help.js";

function createMockHelpOverlay(initiallyHidden = true) {
    const classes = new Set(initiallyHidden ? ["d-none"] : []);
    const attributes = new Map();

    return {
        classList: {
            contains(name) {
                return classes.has(name);
            },
            add(name) {
                classes.add(name);
            },
            remove(name) {
                classes.delete(name);
            }
        },
        setAttribute(name, value) {
            attributes.set(name, value);
        },
        getAttribute(name) {
            return attributes.get(name);
        }
    };
}

test("buildShortcutReferenceText includes H help toggle shortcut", () => {
    const text = buildShortcutReferenceText();
    assert.match(text, /H = Toggle Help/);
    assert.match(text, /F12 = Fullscreen/);
});

test("buildShortcutOverlayText renders multiline list for overlay", () => {
    const text = buildShortcutOverlayText();

    assert.match(text, /^Shortcuts/m);
    assert.match(text, /H: Toggle Help/);
    assert.match(text, /F12: Fullscreen/);
    assert.match(text, /\n/);
});

test("computeShortcutHelpFontSizePx scales from canvas height", () => {
    assert.equal(computeShortcutHelpFontSizePx(960), 26);
    assert.equal(computeShortcutHelpFontSizePx(480), 13);
});

test("computeShortcutHelpFontSizePx clamps to supported range", () => {
    assert.equal(computeShortcutHelpFontSizePx(120), 12);
    assert.equal(computeShortcutHelpFontSizePx(2200), 38);
});

test("WEB_SHORTCUT_REFERENCE includes KeyH mapping", () => {
    const helpShortcut = WEB_SHORTCUT_REFERENCE.find((shortcut) => shortcut.key === "H");
    assert.deepEqual(helpShortcut, { key: "H", action: "Toggle Help" });
});

test("toggleShortcutHelpVisibility shows hidden overlay", () => {
    const overlay = createMockHelpOverlay(true);

    const visible = toggleShortcutHelpVisibility(overlay);

    assert.equal(visible, true);
    assert.equal(overlay.classList.contains("d-none"), false);
    assert.equal(overlay.getAttribute("aria-hidden"), "false");
});

test("toggleShortcutHelpVisibility hides visible overlay", () => {
    const overlay = createMockHelpOverlay(false);

    const visible = toggleShortcutHelpVisibility(overlay);

    assert.equal(visible, false);
    assert.equal(overlay.classList.contains("d-none"), true);
    assert.equal(overlay.getAttribute("aria-hidden"), "true");
});
