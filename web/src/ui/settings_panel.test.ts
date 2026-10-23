/**
 * @vitest-environment jsdom
 */
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { beforeEach, describe, expect, it, vi } from "vitest";

import { createSettingsPanel, gamepadLines, keyBindingRows, renderKeyList } from "./settings_panel";

const webDir = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const indexHtml = readFileSync(resolve(webDir, "index.html"), "utf8");
const page = () => new DOMParser().parseFromString(indexHtml, "text/html");

describe("the settings panel's markup", () => {
    it("Given the top bar, When reading it, Then a Settings button opens the panel", () => {
        const button = page().querySelector("header #settings-open")!;
        expect(button.getAttribute("aria-label")).toBe("Settings");
        expect(button.getAttribute("aria-controls")).toBe("settings-panel");
        expect(button.getAttribute("aria-haspopup")).toBe("dialog");
    });

    it("Given the panel, When reading it, Then it is a dialog titled Settings with General, Controls and Developer tabs", () => {
        const doc = page();
        const panel = doc.getElementById("settings-panel")!;
        expect(panel.tagName).toBe("DIALOG");
        expect(doc.getElementById(panel.getAttribute("aria-labelledby")!)!.textContent).toBe("Settings");
        expect([...panel.querySelectorAll('[role="tab"]')].map((t) => t.textContent!.trim())).toEqual(["General", "Controls", "Developer"]);
        expect(panel.querySelector("#settings-close")!.getAttribute("aria-label")).toBe("Close settings");
    });

    it("Given the panel's tabs, When reading where things live, Then Game Boy and firmware are General, keys are Controls, autorun and the debugger are Developer", () => {
        const doc = page();
        const tabPanel = (name: string) => doc.getElementById(`settings-tab-${name}`)!;
        expect(tabPanel("general").contains(doc.getElementById("gb-hardware"))).toBe(true);
        expect(doc.querySelector('label[for="gb-hardware"]')!.textContent).toBe("Game Boy games run on");
        expect(tabPanel("general").contains(doc.getElementById("snes-firmware-section"))).toBe(true);
        expect(tabPanel("controls").contains(doc.getElementById("settings-keys"))).toBe(true);
        expect(tabPanel("developer").contains(doc.getElementById("autorun-section"))).toBe(true);
        expect(tabPanel("developer").contains(doc.getElementById("settings-debugger"))).toBe(true);
    });

    it("Given the library, When reading it, Then the settings are not there", () => {
        expect(page().querySelector("#library #gb-hardware, #library #autorun-section, #library #snes-firmware-section")).toBeNull();
    });
});

describe("createSettingsPanel", () => {
    beforeEach(() => {
        document.documentElement.innerHTML = page().documentElement.innerHTML;
    });

    function panel() {
        const onOpen = vi.fn();
        const onClose = vi.fn();
        const dialog = document.getElementById("settings-panel") as HTMLDialogElement;
        const controller = createSettingsPanel({
            dialog,
            openButton: document.getElementById("settings-open") as HTMLButtonElement,
            closeButton: document.getElementById("settings-close") as HTMLButtonElement,
            onOpen,
            onClose,
        });
        return { dialog, controller, onOpen, onClose };
    }

    const tab = (name: string) => [...document.querySelectorAll('#settings-panel [role="tab"]')].find((t) => t.textContent!.trim() === name) as HTMLElement;

    it("Given a closed panel, When Settings is pressed, Then it opens on General and says so once", () => {
        const { dialog, onOpen } = panel();
        (document.getElementById("settings-open") as HTMLButtonElement).click();
        expect(dialog.open).toBe(true);
        expect(onOpen).toHaveBeenCalledTimes(1);
        expect(tab("General").getAttribute("aria-selected")).toBe("true");
        expect(document.getElementById("settings-tab-general")!.hidden).toBe(false);
        expect(document.getElementById("settings-tab-developer")!.hidden).toBe(true);
    });

    it("Given the open panel, When another tab is pressed, Then only its page shows", () => {
        const { controller } = panel();
        controller.open();
        tab("Developer").click();
        expect(tab("Developer").getAttribute("aria-selected")).toBe("true");
        expect(tab("General").getAttribute("aria-selected")).toBe("false");
        expect(document.getElementById("settings-tab-developer")!.hidden).toBe(false);
        expect(document.getElementById("settings-tab-general")!.hidden).toBe(true);
    });

    it("Given the open panel, When the close button is pressed, Then it closes and says so once, even when the dialog then reports closing", () => {
        const { dialog, controller, onClose } = panel();
        controller.open();
        (document.getElementById("settings-close") as HTMLButtonElement).click();
        dialog.dispatchEvent(new Event("close"));
        expect(dialog.open).toBe(false);
        expect(onClose).toHaveBeenCalledTimes(1);
    });

    it("Given the open panel, When the dialog closes by itself (Esc), Then the close is reported", () => {
        const { dialog, controller, onClose } = panel();
        controller.open();
        dialog.removeAttribute("open");
        dialog.dispatchEvent(new Event("close"));
        expect(onClose).toHaveBeenCalledTimes(1);
    });

    it("Given focus on a tab, When the arrow keys, Home or End are pressed, Then the next tab is chosen and focused, wrapping round", () => {
        const { controller } = panel();
        controller.open();
        const press = (key: string) => document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
        tab("General").focus();
        press("ArrowRight");
        expect(document.activeElement).toBe(tab("Controls"));
        expect(tab("Controls").getAttribute("aria-selected")).toBe("true");
        expect(document.getElementById("settings-tab-controls")!.hidden).toBe(false);
        press("End");
        expect(document.activeElement).toBe(tab("Developer"));
        press("ArrowRight");
        expect(document.activeElement).toBe(tab("General"));
        press("ArrowLeft");
        expect(document.activeElement).toBe(tab("Developer"));
        press("Home");
        expect(document.activeElement).toBe(tab("General"));
        expect(tab("General").tabIndex).toBe(0);
        expect(tab("Developer").tabIndex).toBe(-1);
    });

    it("Given the panel was closed on Developer, When it opens again, Then it opens on General", () => {
        const { controller } = panel();
        controller.open();
        tab("Developer").click();
        controller.close();
        controller.open();
        expect(tab("General").getAttribute("aria-selected")).toBe("true");
    });
});

describe("the key list", () => {
    it("Given a console's key text, When split into rows, Then each row names the button and its keys", () => {
        expect(keyBindingRows("W/A/S/D: D-Pad\nR: A\n4: Select")).toEqual([
            { action: "D-Pad", keys: ["W", "A", "S", "D"] },
            { action: "A", keys: ["R"] },
            { action: "Select", keys: ["4"] },
        ]);
    });

    it("Given a list element, When rendered, Then each button gets a row with one key cap per key", () => {
        const list = document.createElement("div");
        renderKeyList(list, "W/A/S/D: D-Pad\nT: B");
        const rows = [...list.querySelectorAll(".settings-key-row")];
        expect(rows.map((r) => r.querySelector(".settings-key-action")!.textContent)).toEqual(["D-Pad", "B"]);
        expect([...rows[0].querySelectorAll("kbd")].map((k) => k.textContent)).toEqual(["W", "A", "S", "D"]);
    });
});

describe("the gamepads line", () => {
    const pad = (id: string, connected = true) => ({ id, connected }) as Gamepad;

    it("Given no connected gamepad, When described, Then it says how to connect one", () => {
        expect(gamepadLines([null, pad("Old pad", false)])).toBe("No gamepad connected. Plug one in and press any button.");
    });

    it("Given connected gamepads, When described, Then the first two are players 1 and 2, as the game reads them", () => {
        expect(gamepadLines([pad("Pad A"), null, pad("Pad B"), pad("Pad C")])).toBe("Player 1: Pad A\nPlayer 2: Pad B");
    });
});
