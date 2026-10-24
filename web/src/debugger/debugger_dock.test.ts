/**
 * @vitest-environment jsdom
 */
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

import { DEBUGGER_BUTTON_STORAGE_KEY, readDebuggerButton, writeDebuggerButton } from "./debugger_dock";

const webDir = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const page = () => new DOMParser().parseFromString(readFileSync(resolve(webDir, "index.html"), "utf8"), "text/html");

describe("the docked debugger's markup", () => {
    it("Given the game view, When reading where the debugger sits, Then it is beside the picture, not over it", () => {
        const doc = page();
        const panel = doc.getElementById("debugger-panel")!;
        expect(panel.closest("#screen-wrap")).toBeNull();
        expect(panel.closest("#game-view")).not.toBeNull();
        expect(panel.previousElementSibling!.classList.contains("game-stage")).toBe(true);
    });

    it("Given the top bar, When reading it, Then a bug button for the debugger waits there, hidden until switched on", () => {
        const button = page().querySelector("header #debugger-open")!;
        expect(button.getAttribute("aria-label")).toBe("Debugger");
        expect(button.getAttribute("aria-keyshortcuts")).toBe("F5");
        expect(button.classList.contains("hidden")).toBe(true);
    });

    it("Given Settings › Developer, When reading the debugger group, Then it has the switch and the words as drawn", () => {
        const doc = page();
        const toggle = doc.getElementById("settings-debugger-button") as HTMLInputElement;
        expect(toggle.type).toBe("checkbox");
        expect(toggle.closest("#settings-tab-developer")).not.toBeNull();
        expect(doc.querySelector('label[for="settings-debugger-button"]')!.textContent).toBe("Show debugger button");
        expect(doc.getElementById("settings-debugger-hint")!.textContent).toBe("Or press F5 at any time");
        expect(doc.getElementById("settings-debugger")).toBeNull();
    });
});

describe("the debugger button switch, remembered", () => {
    const memory = () => {
        const store = new Map<string, string>();
        return { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => void store.set(k, v), store };
    };

    it("Given a first visit, When read, Then the button is off", () => {
        expect(readDebuggerButton(memory())).toBe(false);
        expect(readDebuggerButton(null)).toBe(false);
    });

    it("Given the switch is turned on, When read back, Then it is on", () => {
        const storage = memory();
        writeDebuggerButton(storage, true);
        expect(storage.store.get(DEBUGGER_BUTTON_STORAGE_KEY)).toBe("on");
        expect(readDebuggerButton(storage)).toBe(true);
        writeDebuggerButton(storage, false);
        expect(readDebuggerButton(storage)).toBe(false);
    });

    it("Given storage that refuses, When read or written, Then it is off and nothing throws", () => {
        const refusing = { getItem: () => { throw new Error("no"); }, setItem: () => { throw new Error("no"); } };
        expect(readDebuggerButton(refusing)).toBe(false);
        expect(() => writeDebuggerButton(refusing, true)).not.toThrow();
    });
});
