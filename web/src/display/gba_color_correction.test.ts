import { describe, expect, it } from "vitest";
import { gbaColorButtonLabel } from "./gba_color_correction";

describe("gbaColorButtonLabel", () => {
    it("names the raw state when correction is off", () => {
        expect(gbaColorButtonLabel(false)).toBe("Colors: Raw");
    });

    it("names the GBA screen state when correction is on", () => {
        expect(gbaColorButtonLabel(true)).toBe("Colors: GBA screen");
    });
});
