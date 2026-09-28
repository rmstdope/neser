import { describe, it, expect } from "vitest";
import { buttonFilterKey, filterFailed, filterReady, requestFilter, selectionAt } from "./filter_selection";

describe("filter selection while a look loads", () => {
    it("starts with the button and the picture on the same look", () => {
        const sel = selectionAt("stock");
        expect(sel.shown).toBe("stock");
        expect(buttonFilterKey(sel)).toBe("stock");
    });

    it("moves the button at once and keeps the picture until the look is ready", () => {
        const sel = requestFilter(selectionAt("nsoGbaColor"), "sp101Color");
        expect(buttonFilterKey(sel)).toBe("sp101Color");
        expect(sel.shown).toBe("nsoGbaColor");
        const ready = filterReady(sel, "sp101Color");
        expect(ready.shown).toBe("sp101Color");
        expect(buttonFilterKey(ready)).toBe("sp101Color");
    });

    it("puts the button back on the picture's look when the look cannot be fetched", () => {
        const sel = filterFailed(requestFilter(selectionAt("nsoGbaColor"), "sp101Color"), "sp101Color");
        expect(sel).toEqual({ shown: "nsoGbaColor", requested: "nsoGbaColor" });
    });

    it("ignores a look that finishes loading after a newer press", () => {
        let sel = requestFilter(selectionAt("nsoGbaColor"), "sp101Color");
        sel = requestFilter(sel, "gbaLcdGrid");
        expect(filterReady(sel, "sp101Color")).toEqual(sel);
        expect(filterFailed(sel, "sp101Color")).toEqual(sel);
        expect(filterReady(sel, "gbaLcdGrid")).toEqual({ shown: "gbaLcdGrid", requested: "gbaLcdGrid" });
    });

    it("ignores a load that finishes after the selection was reset for another game", () => {
        const sel = selectionAt("ntsc");
        expect(filterReady(sel, "sp101Color")).toEqual(sel);
    });
});
