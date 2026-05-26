import { expect, it } from "vitest";

import {
    WEB_SHORTCUT_REFERENCE,
    buildShortcutOverlayText,
    buildShortcutReferenceText,
    buildControllerOverlayText,
    buildFullHelpOverlayText,
    computeShortcutHelpFontSizePx,
    toggleShortcutHelpVisibility
} from "./shortcut_help";

function createMockHelpOverlay(initiallyHidden = true) {
    const classes = new Set(initiallyHidden ? ["hidden"] : []);
    const attributes = new Map();

    return {
        classList: {
            contains(name: string) {
                return classes.has(name);
            },
            add(name: string) {
                classes.add(name);
            },
            remove(name: string) {
                classes.delete(name);
            }
        },
        setAttribute(name: string, value: string) {
            attributes.set(name, value);
        },
        getAttribute(name: string) {
            return attributes.get(name);
        }
    };
}

it("buildShortcutReferenceText includes H help toggle shortcut", () => {
    const text = buildShortcutReferenceText();
    expect(text).toMatch(/H = Toggle Help/);
    expect(text).toMatch(/Ctrl\+F = Toggle Fullscreen/);
    expect(text).toMatch(/Ctrl\+R = Soft Reset/);
    expect(text).toMatch(/Shift\+Ctrl\+R = Hard Reset/);
    expect(text).toMatch(/F4 = Cycle Filter/);
});

it("buildShortcutOverlayText renders multiline list for overlay", () => {
    const text = buildShortcutOverlayText();

    expect(text).toMatch(/^Shortcuts/m);
    expect(text).toMatch(/H: Toggle Help/);
    expect(text).toMatch(/Ctrl\+F: Toggle Fullscreen/);
    expect(text).toMatch(/Ctrl\+R: Soft Reset/);
    expect(text).toMatch(/Shift\+Ctrl\+R: Hard Reset/);
    expect(text).toMatch(/F4: Cycle Filter/);
    expect(text).toMatch(/\n/);
});

it("computeShortcutHelpFontSizePx scales from canvas height", () => {
    expect(computeShortcutHelpFontSizePx(960)).toBe(26);
    expect(computeShortcutHelpFontSizePx(480)).toBe(13);
});

it("computeShortcutHelpFontSizePx clamps to supported range", () => {
    expect(computeShortcutHelpFontSizePx(120)).toBe(12);
    expect(computeShortcutHelpFontSizePx(2200)).toBe(38);
});

it("WEB_SHORTCUT_REFERENCE includes help, soft reset, and hard reset mappings", () => {
    const helpShortcut = WEB_SHORTCUT_REFERENCE.find((shortcut) => shortcut.key === "H");
    expect(helpShortcut).toEqual({ key: "H", action: "Toggle Help" });

    const softResetShortcut = WEB_SHORTCUT_REFERENCE.find(
        (shortcut) => shortcut.key === "Ctrl+R"
    );
    expect(softResetShortcut).toEqual({ key: "Ctrl+R", action: "Soft Reset" });

    const hardResetShortcut = WEB_SHORTCUT_REFERENCE.find(
        (shortcut) => shortcut.key === "Shift+Ctrl+R"
    );
    expect(hardResetShortcut).toEqual({ key: "Shift+Ctrl+R", action: "Hard Reset" });
});

it("toggleShortcutHelpVisibility shows hidden overlay", () => {
    const overlay = createMockHelpOverlay(true);

    const visible = toggleShortcutHelpVisibility(overlay as any);

    expect(visible).toBe(true);
    expect(overlay.classList.contains("hidden")).toBe(false);
    expect(overlay.getAttribute("aria-hidden")).toBe("false");
});

it("toggleShortcutHelpVisibility hides visible overlay", () => {
    const overlay = createMockHelpOverlay(false);

    const visible = toggleShortcutHelpVisibility(overlay as any);

    expect(visible).toBe(false);
    expect(overlay.classList.contains("hidden")).toBe(true);
    expect(overlay.getAttribute("aria-hidden")).toBe("true");
});

// Tests for buildControllerOverlayText
it("buildControllerOverlayText shows NES keyboard keys for both players when no gamepads", () => {
    const text = buildControllerOverlayText(0, "nes");
    expect(text).toMatch(/Controller \(Player 1\)/);
    expect(text).toMatch(/W\/A\/S\/D: D-Pad/);
    expect(text).toMatch(/R: A/);
    expect(text).toMatch(/T: B/);
    expect(text).not.toMatch(/F: B/);
    expect(text).not.toMatch(/G: A/);
    expect(text).not.toMatch(/Q: L/);
    expect(text).not.toMatch(/E: R/);
    expect(text).not.toMatch(/X/);
    expect(text).not.toMatch(/Y/);
    expect(text).toMatch(/4: Select/);
    expect(text).toMatch(/5: Start/);
    expect(text).toMatch(/Controller \(Player 2\)/);
    expect(text).toMatch(/I\/J\/K\/L: D-Pad/);
    expect(text).toMatch(/O: A/);
    expect(text).toMatch(/P: B/);
    expect(text).toMatch(/9: Select/);
    expect(text).toMatch(/0: Start/);
});

it("buildControllerOverlayText shows only one Game Boy controller with A and B", () => {
    const text = buildControllerOverlayText(0, "gb");
    expect(text).toMatch(/Controller \(Player 1\)/);
    expect(text).toMatch(/W\/A\/S\/D: D-Pad/);
    expect(text).toMatch(/R: A/);
    expect(text).toMatch(/T: B/);
    expect(text).toMatch(/4: Select/);
    expect(text).toMatch(/5: Start/);
    expect(text).not.toMatch(/Controller \(Player 2\)/);
    expect(text).not.toMatch(/I\/J\/K\/L/);
    expect(text).not.toMatch(/X/);
    expect(text).not.toMatch(/Y/);
    expect(text).not.toMatch(/Q: L/);
    expect(text).not.toMatch(/E: R/);
});

it("buildControllerOverlayText reserves X, Y, and shoulder keys for AGB", () => {
    const text = buildControllerOverlayText(0, "gba");
    expect(text).toMatch(/Controller \(Player 1\)/);
    expect(text).toMatch(/R: Y/);
    expect(text).toMatch(/T: X/);
    expect(text).toMatch(/F: B/);
    expect(text).toMatch(/G: A/);
    expect(text).toMatch(/Q: L/);
    expect(text).toMatch(/E: R/);
    expect(text).not.toMatch(/Controller \(Player 2\)/);
});

it("buildControllerOverlayText shows Gamepad for player 1 and keyboard for player 2 when one gamepad connected", () => {
    const text = buildControllerOverlayText(1, "nes");
    expect(text).toMatch(/Controller \(Player 1\)/);
    expect(text).toMatch(/Gamepad/);
    expect(text).not.toMatch(/W\/A\/S\/D/);
    expect(text).toMatch(/Controller \(Player 2\)/);
    expect(text).toMatch(/I\/J\/K\/L: D-Pad/);
});

it("buildControllerOverlayText shows Gamepad for both players when two gamepads connected", () => {
    const text = buildControllerOverlayText(2, "nes");
    expect(text).toMatch(/Controller \(Player 1\)/);
    expect(text).toMatch(/Controller \(Player 2\)/);
    expect(text).not.toMatch(/W\/A\/S\/D/);
    expect(text).not.toMatch(/I\/J\/K\/L/);
    const gamepadMatches = [...text.matchAll(/Gamepad/g)];
    expect(gamepadMatches.length).toBe(2);
});

it("buildFullHelpOverlayText uses the selected emulator controller help", () => {
    const text = buildFullHelpOverlayText(0, "gb");
    expect(text).toMatch(/^Shortcuts/m);
    expect(text).toMatch(/Controller \(Player 1\)/);
    expect(text).not.toMatch(/Controller \(Player 2\)/);
});
