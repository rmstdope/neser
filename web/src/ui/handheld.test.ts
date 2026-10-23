/**
 * @vitest-environment jsdom
 */
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

const webDir = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const page = () => new DOMParser().parseFromString(readFileSync(resolve(webDir, "index.html"), "utf8"), "text/html");

describe("the phone layout's markup", () => {
    it("Given the touch controls, When reading them, Then a quick bar with Pause, Save, Load and ⋯ sits with the picture", () => {
        const bar = page().getElementById("handheld-bar")!;
        expect(bar.closest("#touch-controls")).not.toBeNull();
        expect([...bar.querySelectorAll("button")].map((b) => b.id)).toEqual(["hh-pause", "hh-save", "hh-load", "hh-more"]);
        expect(page().getElementById("hh-more")!.getAttribute("aria-label")).toBe("More");
    });

    it("Given the ⋯ sheet, When reading it, Then it holds the rest: reset, hard reset, fullscreen, eject, filter, palette, volume, settings and the keys", () => {
        const sheet = page().getElementById("handheld-sheet")!;
        expect(sheet.tagName).toBe("DIALOG");
        expect([...sheet.querySelectorAll(".hh-tile")].map((t) => t.textContent!.trim())).toEqual(["Reset", "Hard reset", "Fullscreen", "Eject"]);
        expect(sheet.querySelector("#hh-filter")).not.toBeNull();
        expect(sheet.querySelector("#hh-palette")).not.toBeNull();
        expect((sheet.querySelector("#hh-volume") as HTMLInputElement).type).toBe("range");
        expect(sheet.querySelector("#hh-settings")!.textContent!.trim()).toBe("Settings");
        expect(sheet.querySelector("#hh-help")!.textContent!.trim()).toBe("Keys and controls");
    });
});
