/**
 * @vitest-environment jsdom
 */
import { describe, expect, it } from "vitest";

import indexHtml from "../../index.html?raw";

function parseTouchControls(): HTMLElement {
    const documentRoot = new DOMParser().parseFromString(indexHtml, "text/html");
    const touchControls = documentRoot.getElementById("touch-controls");
    if (!touchControls) {
        throw new Error("touch controls container should exist");
    }
    return touchControls;
}

describe("touch controls markup", () => {
    it("Given touch controls markup, When reading the left controls, Then a joystick zone with a knob is present instead of segmented d-pad buttons", () => {
        const touchControls = parseTouchControls();

        expect(touchControls.querySelector('[data-touch-zone="joystick"]')).not.toBeNull();
        expect(touchControls.querySelector(".touch-joystick-knob")).not.toBeNull();
        expect(touchControls.querySelector(".touch-dpad-btn")).toBeNull();
    });

    it("Given touch controls markup, When reading the action area, Then A and B remain direct buttons without a shared action zone", () => {
        const touchControls = parseTouchControls();

        expect(touchControls.querySelector('[data-touch-zone="actions"]')).toBeNull();
        expect(touchControls.querySelector('.touch-btn-a[data-button="a"]')).not.toBeNull();
        expect(touchControls.querySelector('.touch-btn-b[data-button="b"]')).not.toBeNull();
        expect(touchControls.querySelector('.touch-meta-btn[data-button="select"]')).not.toBeNull();
        expect(touchControls.querySelector('.touch-meta-btn[data-button="start"]')).not.toBeNull();
    });
});