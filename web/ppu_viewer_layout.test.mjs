import test from "node:test";
import assert from "node:assert/strict";
import {
    computeNtscDisplayWidth,
    computeScrollViewportRects,
} from "./ppu_viewer_layout.js";

test("computeNtscDisplayWidth converts NES width using NTSC 8:7 aspect", () => {
    assert.equal(computeNtscDisplayWidth(256), 293);
    assert.equal(computeNtscDisplayWidth(512), 585);
});

test("computeNtscDisplayWidth returns 0 for invalid or non-positive widths", () => {
    assert.equal(computeNtscDisplayWidth(0), 0);
    assert.equal(computeNtscDisplayWidth(-1), 0);
    assert.equal(computeNtscDisplayWidth(Number.NaN), 0);
});

test("computeScrollViewportRects returns one rect when no wrapping occurs", () => {
    const rects = computeScrollViewportRects(32, 40);
    assert.equal(rects.length, 1);
    assert.deepEqual(rects[0], {
        x: 32,
        y: 40,
        width: 256,
        height: 240,
    });
});

test("computeScrollViewportRects splits horizontally when x wraps", () => {
    const rects = computeScrollViewportRects(500, 0);
    assert.equal(rects.length, 2);
    assert.deepEqual(rects[0], { x: 500, y: 0, width: 12, height: 240 });
    assert.deepEqual(rects[1], { x: 0, y: 0, width: 244, height: 240 });
});

test("computeScrollViewportRects splits into four rectangles when x and y wrap", () => {
    const rects = computeScrollViewportRects(500, 470);
    assert.equal(rects.length, 4);
    assert.deepEqual(rects[0], { x: 500, y: 470, width: 12, height: 10 });
    assert.deepEqual(rects[1], { x: 500, y: 0, width: 12, height: 230 });
    assert.deepEqual(rects[2], { x: 0, y: 470, width: 244, height: 10 });
    assert.deepEqual(rects[3], { x: 0, y: 0, width: 244, height: 230 });
});
