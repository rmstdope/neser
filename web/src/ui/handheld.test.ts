/**
 * @vitest-environment jsdom
 */
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

import { phoneFpsChip, sheetDragCloses } from "./handheld";

const webDir = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const page = () => new DOMParser().parseFromString(readFileSync(resolve(webDir, "index.html"), "utf8"), "text/html");

describe("the phone layout's markup", () => {
    it("Given the touch controls, When reading them, Then a quick bar with Pause, Save, Load and ⋯ sits with the picture", () => {
        const bar = page().getElementById("handheld-bar")!;
        expect(bar.closest("#touch-controls")).not.toBeNull();
        expect([...bar.querySelectorAll("button")].map((b) => b.id)).toEqual(["hh-pause", "hh-save", "hh-load", "hh-more"]);
        expect(page().getElementById("hh-more")!.getAttribute("aria-label")).toBe("More");
        expect(page().getElementById("hh-more")!.getAttribute("aria-expanded")).toBe("false");
        // A plain group: it has no arrow-key roving of its own, so it does not claim to be a toolbar.
        expect(bar.getAttribute("role")).toBe("group");
    });

    it("Given the ⋯ sheet, When reading it, Then it holds the rest: reset, hard reset, fullscreen, eject, filter, palette, volume, settings and the keys", () => {
        const sheet = page().getElementById("handheld-sheet")!;
        expect(sheet.tagName).toBe("DIALOG");
        expect([...sheet.querySelectorAll(".hh-tile")].map((t) => t.textContent!.trim())).toEqual(["Reset", "Hard reset", "Fullscreen", "Eject"]);
        expect(sheet.querySelector("#hh-filter")).not.toBeNull();
        expect(sheet.querySelector("#hh-palette")).not.toBeNull();
        expect((sheet.querySelector("#hh-volume") as HTMLInputElement).type).toBe("range");
        expect(sheet.querySelector("#hh-settings .hh-row-label")!.textContent).toBe("Settings");
        expect(sheet.querySelector("#hh-help .hh-row-label")!.textContent).toBe("Keys and controls");
    });

    it("Given the sheet's rows, When reading them, Then they are as drawn: volume an icon and a slider, chevrons on Palette, Settings and the keys", () => {
        const sheet = page().getElementById("handheld-sheet")!;
        const volumeRow = sheet.querySelector("#hh-volume")!.closest(".hh-row")!;
        expect(volumeRow.textContent!.trim()).toBe("");
        expect(sheet.querySelector("#hh-volume")!.getAttribute("aria-label")).toBe("Volume");
        for (const id of ["hh-palette", "hh-settings", "hh-help"]) expect(sheet.querySelector(`#${id} .hh-chevron`), id).not.toBeNull();
    });
});

describe("phoneFpsChip", () => {
    it("Given a phone, When the frame rate is shown, Then it reads as drawn: the whole number, or Paused", () => {
        expect(phoneFpsChip(59.6, false)).toBe("60");
        expect(phoneFpsChip(18.2, false)).toBe("18");
        expect(phoneFpsChip(60, true)).toBe("Paused");
    });
});

describe("sheetDragCloses", () => {
    it("Given the sheet dragged, When let go, Then it closes once pulled well down, and springs back otherwise", () => {
        expect(sheetDragCloses(90)).toBe(true);
        expect(sheetDragCloses(30)).toBe(false);
        expect(sheetDragCloses(-50)).toBe(false);
    });
});
