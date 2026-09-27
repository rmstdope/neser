import { describe, expect, it } from "vitest";
import {
    cgbColorButtonLabel,
    cgbColorButtonVisible,
    cgbColorToastMessage,
    createCgbColorControl
} from "./cgb_color_correction";

describe("cgbColorButtonLabel", () => {
    it("names the raw state when correction is off", () => {
        expect(cgbColorButtonLabel(false)).toBe("Colors: Raw");
    });

    it("names the GBC screen state when correction is on", () => {
        expect(cgbColorButtonLabel(true)).toBe("Colors: GBC screen");
    });
});

describe("cgbColorToastMessage", () => {
    it("shows the GBC screen message when correction is switched on", () => {
        expect(cgbColorToastMessage(true)).toBe("Colors: GBC screen");
    });

    it("shows the raw message when correction is switched off", () => {
        expect(cgbColorToastMessage(false)).toBe("Colors: Raw");
    });
});

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
            cgb_color_correction: () => coreState
        };
        const control = createCgbColorControl({
            refreshButton: () => events.push(`button:${cgbColorButtonLabel(control.enabled())}`),
            showMessage: (message) => events.push(`message:${message}`)
        });
        return { control, core, events, setCore: (enabled: boolean) => (coreState = enabled) };
    }

    it("starts at Raw on each page load", () => {
        expect(setup().control.enabled()).toBe(false);
    });

    it("a button click switches the running game, relabels and shows the message", () => {
        const { control, core, events } = setup();
        control.click(core);
        expect(core.cgb_color_correction()).toBe(true);
        expect(events).toEqual(["button:Colors: GBC screen", "message:Colors: GBC screen"]);
        control.click(core);
        expect(core.cgb_color_correction()).toBe(false);
        expect(events.slice(2)).toEqual(["button:Colors: Raw", "message:Colors: Raw"]);
    });

    it("a button click without a Game Boy game still switches the page's state", () => {
        const { control } = setup();
        control.click(null);
        expect(control.enabled()).toBe(true);
    });

    it("after F8 it follows the game's state and relabels, leaving the message to F8", () => {
        const { control, core, events, setCore } = setup();
        setCore(true);
        control.afterF8(core);
        expect(control.enabled()).toBe(true);
        expect(events).toEqual(["button:Colors: GBC screen"]);
    });

    it("a click after F8 switches from the state F8 chose", () => {
        const { control, core, setCore } = setup();
        setCore(true);
        control.afterF8(core);
        control.click(core);
        expect(core.cgb_color_correction()).toBe(false);
    });
});
