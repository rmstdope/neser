/**
 * @vitest-environment jsdom
 */
import { describe, expect, it } from "vitest";

import { belongsToOpenDialog, isEditableKeyboardTarget } from "./keyboard_target";

describe("keyboard targets the game must not take keys from", () => {
    it("Given a text field or a select, When a key goes to it, Then it is editable", () => {
        expect(isEditableKeyboardTarget(document.createElement("input"))).toBe(true);
        expect(isEditableKeyboardTarget(document.createElement("select"))).toBe(true);
        expect(isEditableKeyboardTarget(document.createElement("button"))).toBe(false);
        expect(isEditableKeyboardTarget(null)).toBe(false);
    });

    it("Given an open dialog, When a key goes to anything inside it, Then the key is the dialog's", () => {
        document.body.innerHTML = '<dialog open><button id="inside">Tab</button></dialog><dialog><button id="closed">x</button></dialog><button id="outside">x</button>';
        expect(belongsToOpenDialog(document.getElementById("inside"))).toBe(true);
        expect(belongsToOpenDialog(document.getElementById("closed"))).toBe(false);
        expect(belongsToOpenDialog(document.getElementById("outside"))).toBe(false);
        expect(belongsToOpenDialog(document)).toBe(false);
    });
});
