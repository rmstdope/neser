import { expect, it } from "vitest";
import {
    computeNtscDisplayWidth,
    computeScrollViewportRects,
} from "./ppu_viewer_layout";

it("computeNtscDisplayWidth converts NES width using NTSC 8:7 aspect", () => {
    expect(computeNtscDisplayWidth(256)).toBe(293);
    expect(computeNtscDisplayWidth(512)).toBe(585);
});

it("computeNtscDisplayWidth returns 0 for invalid or non-positive widths", () => {
    expect(computeNtscDisplayWidth(0)).toBe(0);
    expect(computeNtscDisplayWidth(-1)).toBe(0);
    expect(computeNtscDisplayWidth(Number.NaN)).toBe(0);
});

it("computeScrollViewportRects returns one rect when no wrapping occurs", () => {
    const rects = computeScrollViewportRects(32, 40);
    expect(rects.length).toBe(1);
    expect(rects[0]).toEqual({
        x: 32,
        y: 40,
        width: 256,
        height: 240,
    });
});

it("computeScrollViewportRects splits horizontally when x wraps", () => {
    const rects = computeScrollViewportRects(500, 0);
    expect(rects.length).toBe(2);
    expect(rects[0]).toEqual({ x: 500, y: 0, width: 12, height: 240 });
    expect(rects[1]).toEqual({ x: 0, y: 0, width: 244, height: 240 });
});

it("computeScrollViewportRects splits into four rectangles when x and y wrap", () => {
    const rects = computeScrollViewportRects(500, 470);
    expect(rects.length).toBe(4);
    expect(rects[0]).toEqual({ x: 500, y: 470, width: 12, height: 10 });
    expect(rects[1]).toEqual({ x: 500, y: 0, width: 12, height: 230 });
    expect(rects[2]).toEqual({ x: 0, y: 470, width: 244, height: 10 });
    expect(rects[3]).toEqual({ x: 0, y: 0, width: 244, height: 230 });
});
