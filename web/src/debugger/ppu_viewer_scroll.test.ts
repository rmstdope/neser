import { expect, it } from "vitest";
import { clampScrollTop, sanitizeScrollTop } from "./ppu_viewer_scroll";

it("sanitizeScrollTop normalizes invalid values to zero", () => {
    expect(sanitizeScrollTop(undefined as unknown as number)).toBe(0);
    expect(sanitizeScrollTop(Number.NaN)).toBe(0);
    expect(sanitizeScrollTop(-5)).toBe(0);
});

it("sanitizeScrollTop preserves finite non-negative values", () => {
    expect(sanitizeScrollTop(0)).toBe(0);
    expect(sanitizeScrollTop(42)).toBe(42);
});

it("clampScrollTop keeps value within scrollable range", () => {
    expect(clampScrollTop(50, { scrollHeight: 300, clientHeight: 100 })).toBe(50);
    expect(clampScrollTop(500, { scrollHeight: 300, clientHeight: 100 })).toBe(200);
    expect(clampScrollTop(-10, { scrollHeight: 300, clientHeight: 100 })).toBe(0);
});

it("clampScrollTop handles non-scrollable containers", () => {
    expect(clampScrollTop(50, { scrollHeight: 100, clientHeight: 100 })).toBe(0);
});
