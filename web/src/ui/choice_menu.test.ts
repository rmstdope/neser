/**
 * @vitest-environment jsdom
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

import { closeOpenChoiceMenus, createChoiceMenu, type Choice } from "./choice_menu";

describe("createChoiceMenu", () => {
    let anchor: HTMLButtonElement;
    let choices: Choice[];
    let onPick: ReturnType<typeof vi.fn<(index: number) => void>>;

    beforeEach(() => {
        document.body.innerHTML = '<div class="host"><button id="anchor">Filter: NTSC</button></div><button id="elsewhere">x</button>';
        anchor = document.getElementById("anchor") as HTMLButtonElement;
        choices = [
            { label: "None", selected: false },
            { label: "NTSC", selected: true, note: "default" },
            { label: "CRT", selected: false },
        ];
        onPick = vi.fn<(index: number) => void>();
    });

    const make = (extra: Partial<Parameters<typeof createChoiceMenu>[0]> = {}) =>
        createChoiceMenu({ anchor, title: "Filter · NES", choices: () => choices, onPick, footer: "Cycle with", footerKeys: ["F4"], ...extra });
    const menu = () => document.querySelector<HTMLElement>(".choice-menu");
    const items = () => [...document.querySelectorAll<HTMLButtonElement>(".choice-menu [role='menuitemradio']")];

    it("Given a closed menu, When its button is pressed, Then it lists every choice with the current one ticked, its note, a title and a footer", () => {
        make();
        anchor.click();
        expect(menu()).not.toBeNull();
        expect(anchor.getAttribute("aria-expanded")).toBe("true");
        expect(items().map((i) => i.querySelector(".choice-menu-label")!.textContent)).toEqual(["None", "NTSC", "CRT"]);
        expect(items().map((i) => i.getAttribute("aria-checked"))).toEqual(["false", "true", "false"]);
        expect(items()[1].querySelector(".choice-menu-note")!.textContent).toBe("default");
        expect(menu()!.querySelector(".choice-menu-title")!.textContent).toBe("Filter · NES");
        expect(menu()!.querySelector(".choice-menu-footer")!.textContent).toBe("Cycle with F4");
        expect(document.activeElement).toBe(items()[1]);
    });

    it("Given the open menu, When a choice is pressed, Then it is picked, the menu closes and focus returns to the button", () => {
        make();
        anchor.click();
        items()[2].click();
        expect(onPick).toHaveBeenCalledWith(2);
        expect(menu()).toBeNull();
        expect(anchor.getAttribute("aria-expanded")).toBe("false");
        expect(document.activeElement).toBe(anchor);
    });

    it("Given the open menu, When the arrow keys are pressed, Then focus moves between the choices and wraps", () => {
        make();
        anchor.click();
        const key = (k: string) => document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true }));
        key("ArrowDown");
        expect(document.activeElement).toBe(items()[2]);
        key("ArrowDown");
        expect(document.activeElement).toBe(items()[0]);
        key("ArrowUp");
        expect(document.activeElement).toBe(items()[2]);
    });

    it("Given the open menu, When Esc is pressed, Then it closes without picking and focus returns to the button", () => {
        make();
        anchor.click();
        document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
        expect(menu()).toBeNull();
        expect(onPick).not.toHaveBeenCalled();
        expect(document.activeElement).toBe(anchor);
    });

    it("Given the open menu, When something else on the page is pressed, Then it closes without picking", () => {
        make();
        anchor.click();
        document.getElementById("elsewhere")!.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true }));
        expect(menu()).toBeNull();
        expect(onPick).not.toHaveBeenCalled();
    });

    it("Given the open menu, When its button is pressed again, Then it closes", () => {
        make();
        anchor.click();
        anchor.click();
        expect(menu()).toBeNull();
    });

    it("Given plain actions rather than settings, When listed, Then they are menu items with their keys and no ticks", () => {
        choices = [
            { label: "Soft reset", selected: false, keys: ["Ctrl", "R"] },
            { label: "Hard reset", selected: false, keys: ["⇧", "Ctrl", "R"] },
        ];
        make({ kind: "actions", title: undefined, footer: undefined, footerKeys: undefined });
        anchor.click();
        const actions = [...document.querySelectorAll<HTMLButtonElement>(".choice-menu [role='menuitem']")];
        expect(actions).toHaveLength(2);
        expect([...actions[1].querySelectorAll("kbd")].map((k) => k.textContent)).toEqual(["⇧", "Ctrl", "R"]);
        expect(document.activeElement).toBe(actions[0]);
    });

    it("Given an open list, When something else acts (a shortcut, the game pausing), Then every open list closes without picking", () => {
        make();
        anchor.click();
        closeOpenChoiceMenus();
        expect(menu()).toBeNull();
        expect(onPick).not.toHaveBeenCalled();
        expect(anchor.getAttribute("aria-expanded")).toBe("false");
    });

    it("Given a footer with no keys, When drawn, Then it is just the words", () => {
        make({ footer: "Hard reset is the same as power off and on.", footerKeys: undefined });
        anchor.click();
        expect(menu()!.querySelector(".choice-menu-footer")!.textContent).toBe("Hard reset is the same as power off and on.");
    });
});
