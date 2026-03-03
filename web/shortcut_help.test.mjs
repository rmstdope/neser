import assert from "node:assert/strict";
import test from "node:test";

import {
    WEB_SHORTCUT_REFERENCE,
    buildShortcutOverlayText,
    buildShortcutReferenceText,
    buildControllerOverlayText,
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
    assert.match(text, /Cmd\/Alt\+F = Toggle Fullscreen/);
    assert.match(text, /Cmd\/Alt\+R = Soft Reset/);
    assert.match(text, /Shift\+Cmd\/Alt\+R = Hard Reset/);
    assert.match(text, /F4 = Cycle Filter/);
});

test("buildShortcutOverlayText renders multiline list for overlay", () => {
    const text = buildShortcutOverlayText();

    assert.match(text, /^Shortcuts/m);
    assert.match(text, /H: Toggle Help/);
    assert.match(text, /Cmd\/Alt\+F: Toggle Fullscreen/);
    assert.match(text, /Cmd\/Alt\+R: Soft Reset/);
    assert.match(text, /Shift\+Cmd\/Alt\+R: Hard Reset/);
    assert.match(text, /F4: Cycle Filter/);
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

test("WEB_SHORTCUT_REFERENCE includes help, soft reset, and hard reset mappings", () => {
    const helpShortcut = WEB_SHORTCUT_REFERENCE.find((shortcut) => shortcut.key === "H");
    assert.deepEqual(helpShortcut, { key: "H", action: "Toggle Help" });

    const softResetShortcut = WEB_SHORTCUT_REFERENCE.find(
        (shortcut) => shortcut.key === "Cmd/Alt+R"
    );
    assert.deepEqual(softResetShortcut, { key: "Cmd/Alt+R", action: "Soft Reset" });

    const hardResetShortcut = WEB_SHORTCUT_REFERENCE.find(
        (shortcut) => shortcut.key === "Shift+Cmd/Alt+R"
    );
    assert.deepEqual(hardResetShortcut, { key: "Shift+Cmd/Alt+R", action: "Hard Reset" });
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

// Tests for buildControllerOverlayText
test("buildControllerOverlayText shows keyboard keys for both players when no gamepads", () => {
    const text = buildControllerOverlayText(0);
    assert.match(text, /Controller \(Player 1\)/);
    assert.match(text, /W\/A\/S\/D: D-Pad/);
    assert.match(text, /R: A/);
    assert.match(text, /T: B/);
    assert.match(text, /4: Select/);
    assert.match(text, /5: Start/);
    assert.match(text, /Controller \(Player 2\)/);
    assert.match(text, /I\/J\/K\/L: D-Pad/);
    assert.match(text, /O: A/);
    assert.match(text, /P: B/);
    assert.match(text, /9: Select/);
    assert.match(text, /0: Start/);
});

test("buildControllerOverlayText shows Gamepad for player 1 and keyboard for player 2 when one gamepad connected", () => {
    const text = buildControllerOverlayText(1);
    assert.match(text, /Controller \(Player 1\)/);
    assert.match(text, /Gamepad/);
    assert.doesNotMatch(text, /W\/A\/S\/D/);
    assert.match(text, /Controller \(Player 2\)/);
    assert.match(text, /I\/J\/K\/L: D-Pad/);
});

test("buildControllerOverlayText shows Gamepad for both players when two gamepads connected", () => {
    const text = buildControllerOverlayText(2);
    assert.match(text, /Controller \(Player 1\)/);
    assert.match(text, /Controller \(Player 2\)/);
    assert.doesNotMatch(text, /W\/A\/S\/D/);
    assert.doesNotMatch(text, /I\/J\/K\/L/);
    const gamepadMatches = [...text.matchAll(/Gamepad/g)];
    assert.equal(gamepadMatches.length, 2);
});
