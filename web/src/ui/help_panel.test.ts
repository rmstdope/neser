/**
 * @vitest-environment jsdom
 */
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

import { HELP_SHORTCUTS, renderHelpShortcuts, renderPad } from "./help_panel";

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
            ["H", "These keys and controls"],
        ]);
        expect(HELP_SHORTCUTS).toHaveLength(10);
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
    });

    it("Given the SNES, When the pad is drawn, Then it has four face buttons and both shoulders", () => {
        const pad = document.createElement("div");
        renderPad(pad, "W/A/S/D: D-Pad\nR: B\nT: A\nY: X\nG: Y\nQ: L\nE: R\n4: Select\n5: Start");
        expect(pad.dataset.layout).toBe("four-button");
        for (const button of ["A", "B", "X", "Y", "L", "R"]) expect(pad.querySelector(`[data-button="${button}"]`), button).not.toBeNull();
        expect(keysOf(pad, "L")).toEqual(["Q"]);
    });
});
