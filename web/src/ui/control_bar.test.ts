/**
 * @vitest-environment jsdom
 */
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

const webDir = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const page = new DOMParser().parseFromString(readFileSync(resolve(webDir, "index.html"), "utf8"), "text/html");
const mainCss = readFileSync(resolve(webDir, "main.css"), "utf8");

const BAR_ORDER = [
    "pause", "reset",
    "save-state", "load-state",
    "stop",
    "screen-minus", "screen-plus", "filter-toggle", "palette-cycle", "cgb-color-toggle", "mute", "volume", "fullscreen",
];

describe("the control bar under the picture", () => {
    const bar = page.getElementById("emulation-controls")!;

    it("Given the page, When reading the control bar, Then it is a toolbar under the picture, not in the sidebar or the top bar", () => {
        expect(bar.getAttribute("role")).toBe("toolbar");
        expect(bar.closest("main")).not.toBeNull();
        expect(bar.closest("aside")).toBeNull();
        expect(page.querySelector("main #screen-wrap")!.compareDocumentPosition(bar) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    });

    it("Given the control bar, When reading its controls, Then play and reset come first, save states next, then eject, then picture and sound", () => {
        const ids = [...bar.querySelectorAll("button, input")].map((el) => el.id);
        expect(ids).toEqual(BAR_ORDER);
        // The top bar holds only page-level buttons, never a game control.
        expect([...page.querySelectorAll("header button")].map((b) => b.id)).toEqual(["library-open", "settings-open"]);
        expect(page.querySelector("#library #pause, #library #stop, #library #save-state")).toBeNull();
    });

    it("Given the control bar, When reading the groups, Then thin separators split them", () => {
        expect(bar.querySelectorAll(".control-bar-sep")).toHaveLength(3);
    });

    it("Given a control with a keyboard shortcut, When hovered or focused, Then its tip names the key as the help does", () => {
        const tips: Record<string, [string, string]> = {
            pause: ["Space", "Space"], reset: ["Control+R", "Ctrl+R"], "save-state": ["F6", "F6"], "load-state": ["F7", "F7"],
            "filter-toggle": ["F4", "F4"], "palette-cycle": ["F8", "F8"], "cgb-color-toggle": ["F8", "F8"], fullscreen: ["Control+F", "Ctrl+F"],
        };
        for (const [id, [spec, shown]] of Object.entries(tips)) {
            const el = page.getElementById(id)!;
            expect(el.getAttribute("aria-keyshortcuts"), id).toBe(spec);
            expect(el.dataset.keys, id).toBe(shown);
            expect(el.dataset.tip, id).toBeTruthy();
        }
        expect(mainCss).toMatch(/\[data-tip\]:hover::after/);
        expect(mainCss).toMatch(/\[data-tip\]:focus-visible::after/);
    });

    it("Given the save states and zoom, When reading them, Then they read as drawn: Save, Load, and the zoom level between − and +", () => {
        expect(page.getElementById("save-state")!.textContent).toBe("Save");
        expect(page.getElementById("load-state")!.textContent).toBe("Load");
        const level = page.getElementById("zoom-level")!;
        expect(level.previousElementSibling!.id).toBe("screen-minus");
        expect(level.nextElementSibling!.id).toBe("screen-plus");
    });

    it("Given every control bar button, When drawn, Then it carries an icon", () => {
        for (const el of bar.querySelectorAll("button")) expect(el.dataset.icon, el.id).toBeTruthy();
    });

    it("Given Stop, When reading it, Then it is called Eject", () => {
        expect(page.getElementById("stop")!.textContent!.trim()).toBe("Eject");
        expect(page.getElementById("stop")!.getAttribute("aria-label")).toBe("Eject game");
    });

    it("Given the volume, When reading it, Then it is a 0 to 100 slider beside the mute button", () => {
        const volume = page.getElementById("volume") as HTMLInputElement;
        expect(volume.type).toBe("range");
        expect([volume.min, volume.max]).toEqual(["0", "100"]);
        expect(volume.getAttribute("aria-label")).toBe("Volume");
    });

    it("Given the top bar, When a game runs, Then it has a place for the game's system and name", () => {
        const title = page.querySelector("header #game-title")!;
        expect(title.querySelector("#game-system")).not.toBeNull();
        expect(title.querySelector("#game-name")).not.toBeNull();
        expect(title.querySelector("#game-file")).not.toBeNull();
    });
});
