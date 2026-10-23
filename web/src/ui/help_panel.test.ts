/**
 * @vitest-environment jsdom
 */
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

import { HELP_PAD_WIDTH, HELP_SHORTCUTS, helpChips, helpPadZoom, renderHelpShortcuts, renderPad } from "./help_panel";

const webDir = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const page = () => new DOMParser().parseFromString(readFileSync(resolve(webDir, "index.html"), "utf8"), "text/html");

describe("the keys and controls panel's markup", () => {
    it("Given the page, When reading it, Then a dialog titled Keys and controls replaces the old text box", () => {
        const doc = page();
        const panel = doc.getElementById("help-panel")!;
        expect(panel.tagName).toBe("DIALOG");
        expect(doc.getElementById(panel.getAttribute("aria-labelledby")!)!.textContent).toBe("Keys and controls");
        expect(doc.getElementById("shortcut-help-overlay")).toBeNull();
    });

    it("Given the top bar, When reading it, Then a ? button opens the panel, as H does", () => {
        const button = page().querySelector("header #help-open")!;
        expect(button.getAttribute("aria-label")).toBe("Keys and controls");
        expect(button.getAttribute("aria-keyshortcuts")).toBe("H");
        expect(button.getAttribute("aria-controls")).toBe("help-panel");
        expect(button.getAttribute("aria-expanded")).toBe("false");
    });

    it("Given the controller column, When reading it, Then one row of chips sits above the pad and the drawn gamepad line below it", () => {
        const doc = page();
        expect(doc.getElementById("help-chips")!.getAttribute("role")).toBe("group");
        expect(doc.getElementById("help-gamepad-note")!.textContent!.trim()).toBe("Plug in a gamepad and press any button to use it instead.");
    });
});

describe("helpChips", () => {
    it("Given the NES's first player in view, When the chips are made, Then they read NES · Player 1, Player 2, then the other systems", () => {
        const chips = helpChips("nes", 0, 2);
        expect(chips.map((c) => c.label)).toEqual(["NES · Player 1", "Player 2", "GB", "GBA", "SNES"]);
        expect(chips.map((c) => c.pressed)).toEqual([true, false, false, false, false]);
        expect(chips.map((c) => [c.console, c.player])).toEqual([["nes", 0], ["nes", 1], ["gb", 0], ["gba", 0], ["snes", 0]]);
        expect(chips[2].separated).toBe(true);
    });

    it("Given the second player in view, When the chips are made, Then the system chip names the system and Player 2 is the lit one", () => {
        const chips = helpChips("snes", 1, 2);
        expect(chips.map((c) => c.label)).toEqual(["SNES · Player 1", "Player 2", "NES", "GB", "GBA"]);
        expect(chips.map((c) => c.pressed)).toEqual([false, true, false, false, false]);
    });

    it("Given a one-player system, When the chips are made, Then there is no player chip and its own chip is lit", () => {
        const chips = helpChips("gba", 0, 1);
        expect(chips.map((c) => c.label)).toEqual(["GBA · Player 1", "NES", "GB", "SNES"]);
        expect(chips[0].pressed).toBe(true);
    });
});

describe("renderHelpShortcuts", () => {
    it("Given the shortcuts, When drawn, Then each row shows its keys as caps and what it does, in the drawn words", () => {
        const list = document.createElement("div");
        renderHelpShortcuts(list);
        const rows = [...list.querySelectorAll(".help-shortcut")];
        const read = (row: Element) => [[...row.querySelectorAll("kbd")].map((k) => k.textContent).join(" "), row.querySelector(".help-shortcut-action")!.textContent];
        expect(rows.map(read)).toEqual([
            ["Space", "Pause / resume"],
            ["Ctrl R", "Soft reset"],
            ["⇧ Ctrl R", "Hard reset"],
            ["F6 F7", "Save / load state"],
            ["F4", "Next filter"],
            ["F8", "Next palette / colours"],
            ["Ctrl F", "Fullscreen"],
            ["F5", "Debugger"],
            ["F10 F11", "Step over / into"],
        ]);
        expect(HELP_SHORTCUTS).toHaveLength(9);
    });
});

describe("renderPad", () => {
    const keysOf = (pad: HTMLElement, button: string) =>
        [...pad.querySelectorAll(`[data-button="${button}"] kbd`)].map((k) => k.textContent);

    it("Given the NES's first player, When the pad is drawn, Then every button shows its key", () => {
        const pad = document.createElement("div");
        renderPad(pad, "W/A/S/D: D-Pad\nR: A\nT: B\n4: Select\n5: Start");
        expect(keysOf(pad, "D-Pad")).toEqual(["W", "A", "S", "D"]);
        expect(keysOf(pad, "A")).toEqual(["R"]);
        expect(keysOf(pad, "B")).toEqual(["T"]);
        expect(keysOf(pad, "Select")).toEqual(["4"]);
        expect(keysOf(pad, "Start")).toEqual(["5"]);
        expect(pad.querySelector('[data-button="X"]')).toBeNull();
        expect(pad.dataset.layout).toBe("two-button");
        expect(pad.querySelector('[data-button="A"]')!.getAttribute("role")).toBe("img");
        expect(pad.querySelector('[data-button="A"]')!.getAttribute("aria-label")).toBe("A: R");
    });

    it("Given a player with a gamepad, When the pad is drawn, Then one Gamepad label replaces the keys on every button", () => {
        const pad = document.createElement("div");
        renderPad(pad, "W/A/S/D: D-Pad\nR: A\nT: B\n4: Select\n5: Start", true);
        for (const button of ["D-Pad", "A", "B", "Select", "Start"]) expect(keysOf(pad, button), button).toEqual([]);
        expect([...pad.querySelectorAll(".pad-gamepad kbd")].map((k) => k.textContent)).toEqual(["Gamepad"]);
        expect(pad.querySelector('[data-button="A"]')!.getAttribute("aria-label")).toBe("A: Gamepad");
    });

    it("Given the SNES, When the pad is drawn, Then it has four face buttons and both shoulders", () => {
        const pad = document.createElement("div");
        renderPad(pad, "W/A/S/D: D-Pad\nR: B\nT: A\nY: X\nG: Y\nQ: L\nE: R\n4: Select\n5: Start");
        expect(pad.dataset.layout).toBe("four-button");
        for (const button of ["A", "B", "X", "Y", "L", "R"]) expect(pad.querySelector(`[data-button="${button}"]`), button).not.toBeNull();
        expect(keysOf(pad, "L")).toEqual(["Q"]);
    });
});

describe("helpPadZoom", () => {
    it("Given a column at least as wide as the drawing, When fitted, Then the pad keeps its size", () => {
        expect(helpPadZoom(HELP_PAD_WIDTH)).toBe(1);
        expect(helpPadZoom(900)).toBe(1);
    });

    it("Given a narrower column, When fitted, Then the whole pad scales down to its width", () => {
        expect(helpPadZoom(HELP_PAD_WIDTH / 2)).toBe(0.5);
        expect(helpPadZoom(-5)).toBe(0);
    });
});
