import { describe, expect, it } from "vitest";
import { cgbColorButtonVisible, createCgbColorControl } from "./cgb_color_correction";

describe("cgbColorButtonVisible", () => {
    it("is shown while a colour Game Boy game is running", () => {
        expect(cgbColorButtonVisible({ kind: "gb", isColor: true, running: true, paused: false })).toBe(true);
    });

    it("is hidden when no game is loaded", () => {
        expect(cgbColorButtonVisible({ kind: null, isColor: false, running: false, paused: false })).toBe(false);
    });

    it("is hidden for a Game Boy game shown in black and white", () => {
        expect(cgbColorButtonVisible({ kind: "gb", isColor: false, running: true, paused: false })).toBe(false);
    });

    it("is hidden for NES and SNES games", () => {
        for (const kind of ["nes", "snes"] as const) {
            expect(cgbColorButtonVisible({ kind, isColor: true, running: true, paused: false })).toBe(false);
        }
    });

    it("is shown while a Game Boy Advance game is running", () => {
        expect(cgbColorButtonVisible({ kind: "gba", isColor: false, running: true, paused: false })).toBe(true);
    });

    it("is hidden while a Game Boy Advance game is paused or stopped", () => {
        expect(cgbColorButtonVisible({ kind: "gba", isColor: false, running: true, paused: true })).toBe(false);
        expect(cgbColorButtonVisible({ kind: "gba", isColor: false, running: false, paused: false })).toBe(false);
    });

    it("is hidden while the colour game is paused, since no new frame would show the change", () => {
        expect(cgbColorButtonVisible({ kind: "gb", isColor: true, running: true, paused: true })).toBe(false);
    });

    it("is hidden once the colour game has been stopped", () => {
        expect(cgbColorButtonVisible({ kind: "gb", isColor: true, running: false, paused: false })).toBe(false);
    });
});

describe("createCgbColorControl", () => {
    function setup() {
        const events: string[] = [];
        let coreState = false;
        const core = {
            set_cgb_color_correction: (enabled: boolean) => {
                coreState = enabled;
            },
            cgb_color_correction: () => coreState,
            color_label: () => (coreState ? "core:on" : "core:off")
        };
        const control = createCgbColorControl({
            refreshButton: () => events.push(`button:${core.color_label()}`),
            showMessage: (message) => events.push(`message:${message}`)
        });
        return { control, core, events, setCore: (enabled: boolean) => (coreState = enabled) };
    }

    it("starts at Raw on each page load", () => {
        expect(setup().control.enabled()).toBe(false);
    });

    it("a button click switches the running game, relabels and shows the core's words", () => {
        const { control, core, events } = setup();
        control.click(core);
        expect(core.cgb_color_correction()).toBe(true);
        expect(control.enabled()).toBe(true);
        expect(events).toEqual(["button:core:on", "message:core:on"]);
        control.click(core);
        expect(core.cgb_color_correction()).toBe(false);
        expect(events.slice(2)).toEqual(["button:core:off", "message:core:off"]);
    });

    it("follows the game's state without a message, so F8's choice carries to the next game", () => {
        const { control, core, events, setCore } = setup();
        setCore(true);
        control.follow(core);
        expect(control.enabled()).toBe(true);
        expect(events).toEqual([]);
    });

    it("a click switches from the game's state, even one F8 changed", () => {
        const { control, core, setCore } = setup();
        setCore(true);
        control.click(core);
        expect(core.cgb_color_correction()).toBe(false);
        expect(control.enabled()).toBe(false);
    });
});
