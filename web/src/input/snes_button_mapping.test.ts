import { expect, it } from "vitest";

import { remapLegacySnesButtonId } from "./snes_button_mapping";

it("remaps legacy SNES button IDs to SNES core button IDs", () => {
    expect(remapLegacySnesButtonId(0)).toBe(1);
    expect(remapLegacySnesButtonId(1)).toBe(11);
    expect(remapLegacySnesButtonId(2)).toBe(2);
    expect(remapLegacySnesButtonId(3)).toBe(3);
    expect(remapLegacySnesButtonId(4)).toBe(4);
    expect(remapLegacySnesButtonId(5)).toBe(5);
    expect(remapLegacySnesButtonId(6)).toBe(6);
    expect(remapLegacySnesButtonId(7)).toBe(7);
    expect(remapLegacySnesButtonId(8)).toBe(0);
    expect(remapLegacySnesButtonId(9)).toBe(10);
    expect(remapLegacySnesButtonId(10)).toBe(8);
    expect(remapLegacySnesButtonId(11)).toBe(9);
});

it("passes through unmapped SNES button IDs", () => {
    expect(remapLegacySnesButtonId(42)).toBe(42);
});
