import test from "node:test";
import assert from "node:assert/strict";
import { computeFullscreenCanvasSize, computeWindowedCanvasSize } from "./canvas_size.js";

// NES native resolution (256x240) aspect ratio
const NES_AR = 256 / 240;

test("computeFullscreenCanvasSize - wide viewport letterboxes with full height", () => {
    // 1920x1080 is wider than NES AR (~1.067), so height is maxed out and sides are letterboxed
    const result = computeFullscreenCanvasSize(1920, 1080, NES_AR, 1);
    assert.equal(result.cssHeight, "1080px");
    const expectedWidth = Math.round(1080 * NES_AR);
    assert.equal(result.cssWidth, `${expectedWidth}px`);
    assert.equal(result.pixelHeight, 1080);
    assert.equal(result.pixelWidth, expectedWidth);
});

test("computeFullscreenCanvasSize - portrait viewport letterboxes with full width", () => {
    // 720x1280 is taller than NES AR, so width is maxed and top/bottom are letterboxed
    const result = computeFullscreenCanvasSize(720, 1280, NES_AR, 1);
    assert.equal(result.cssWidth, "720px");
    const expectedHeight = Math.round(720 / NES_AR);
    assert.equal(result.cssHeight, `${expectedHeight}px`);
    assert.equal(result.pixelWidth, 720);
    assert.equal(result.pixelHeight, expectedHeight);
});

test("computeFullscreenCanvasSize - exactly matching aspect ratio fills fully", () => {
    // Viewport matches NES AR exactly: 256x240
    const result = computeFullscreenCanvasSize(256, 240, NES_AR, 1);
    assert.equal(result.cssWidth, "256px");
    assert.equal(result.cssHeight, "240px");
    assert.equal(result.pixelWidth, 256);
    assert.equal(result.pixelHeight, 240);
});

test("computeFullscreenCanvasSize - DPR scales pixel dimensions but not CSS dimensions", () => {
    const dpr = 2;
    const result = computeFullscreenCanvasSize(1920, 1080, NES_AR, dpr);
    // CSS dimensions remain viewport-unit sized
    assert.equal(result.cssHeight, "1080px");
    const expectedCssWidth = Math.round(1080 * NES_AR);
    assert.equal(result.cssWidth, `${expectedCssWidth}px`);
    // Pixel dimensions are DPR-scaled
    assert.equal(result.pixelHeight, Math.round(1080 * dpr));
    assert.equal(result.pixelWidth, Math.round(expectedCssWidth * dpr));
});

test("computeFullscreenCanvasSize - canvas pixel width matches CSS width at DPR 1", () => {
    // Ensures no overcalculation that would cause crosshair/zapper position mismatch
    const result = computeFullscreenCanvasSize(1920, 1080, NES_AR, 1);
    const cssWidth = parseInt(result.cssWidth, 10);
    const cssHeight = parseInt(result.cssHeight, 10);
    assert.equal(result.pixelWidth, cssWidth);
    assert.equal(result.pixelHeight, cssHeight);
});

test("computeWindowedCanvasSize - returns preferred width based on height and aspect ratio", () => {
    const result = computeWindowedCanvasSize(720, NES_AR, 1);
    const expectedWidth = Math.round(720 * NES_AR);
    assert.equal(result.cssWidth, `${expectedWidth}px`);
});

test("computeWindowedCanvasSize - cssHeight is 'auto' to allow responsive CSS width clamping", () => {
    // When max-width:100% clamps the canvas width, height:auto lets it scale proportionally.
    // A fixed pixel height would stretch the image vertically when the window is narrow.
    const result = computeWindowedCanvasSize(720, NES_AR, 1);
    assert.equal(result.cssHeight, "auto");
});

test("computeWindowedCanvasSize - pixel dimensions are set for correct canvas rendering", () => {
    const dpr = 2;
    const result = computeWindowedCanvasSize(720, NES_AR, dpr);
    const expectedCssWidth = Math.round(720 * NES_AR);
    assert.equal(result.pixelWidth, Math.round(expectedCssWidth * dpr));
    assert.equal(result.pixelHeight, Math.round(720 * dpr));
});

test("computeWindowedCanvasSize - height is clamped to valid NES display range", () => {
    const small = computeWindowedCanvasSize(100, NES_AR, 1);
    assert.equal(small.pixelHeight, 240); // min

    const large = computeWindowedCanvasSize(9999, NES_AR, 1);
    assert.equal(large.pixelHeight, 1440); // max
});

