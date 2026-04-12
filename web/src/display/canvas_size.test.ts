import { expect, it } from "vitest";
import { computeFullscreenCanvasSize, computeWindowedCanvasSize } from "./canvas_size";

// NES native resolution (256x240) aspect ratio
const NES_AR = 256 / 240;

it("computeFullscreenCanvasSize - wide viewport letterboxes with full height", () => {
    // 1920x1080 is wider than NES AR (~1.067), so height is maxed out and sides are letterboxed
    const result = computeFullscreenCanvasSize(1920, 1080, NES_AR, 1);
    expect(result.cssHeight).toBe("1080px");
    const expectedWidth = Math.round(1080 * NES_AR);
    expect(result.cssWidth).toBe(`${expectedWidth}px`);
    expect(result.pixelHeight).toBe(1080);
    expect(result.pixelWidth).toBe(expectedWidth);
});

it("computeFullscreenCanvasSize - portrait viewport letterboxes with full width", () => {
    // 720x1280 is taller than NES AR, so width is maxed and top/bottom are letterboxed
    const result = computeFullscreenCanvasSize(720, 1280, NES_AR, 1);
    expect(result.cssWidth).toBe("720px");
    const expectedHeight = Math.round(720 / NES_AR);
    expect(result.cssHeight).toBe(`${expectedHeight}px`);
    expect(result.pixelWidth).toBe(720);
    expect(result.pixelHeight).toBe(expectedHeight);
});

it("computeFullscreenCanvasSize - exactly matching aspect ratio fills fully", () => {
    // Viewport matches NES AR exactly: 256x240
    const result = computeFullscreenCanvasSize(256, 240, NES_AR, 1);
    expect(result.cssWidth).toBe("256px");
    expect(result.cssHeight).toBe("240px");
    expect(result.pixelWidth).toBe(256);
    expect(result.pixelHeight).toBe(240);
});

it("computeFullscreenCanvasSize - DPR scales pixel dimensions but not CSS dimensions", () => {
    const dpr = 2;
    const result = computeFullscreenCanvasSize(1920, 1080, NES_AR, dpr);
    // CSS dimensions remain viewport-unit sized
    expect(result.cssHeight).toBe("1080px");
    const expectedCssWidth = Math.round(1080 * NES_AR);
    expect(result.cssWidth).toBe(`${expectedCssWidth}px`);
    // Pixel dimensions are DPR-scaled
    expect(result.pixelHeight).toBe(Math.round(1080 * dpr));
    expect(result.pixelWidth).toBe(Math.round(expectedCssWidth * dpr));
});

it("computeFullscreenCanvasSize - canvas pixel width matches CSS width at DPR 1", () => {
    // Ensures no overcalculation that would cause crosshair/zapper position mismatch
    const result = computeFullscreenCanvasSize(1920, 1080, NES_AR, 1);
    const cssWidth = parseInt(result.cssWidth, 10);
    const cssHeight = parseInt(result.cssHeight, 10);
    expect(result.pixelWidth).toBe(cssWidth);
    expect(result.pixelHeight).toBe(cssHeight);
});

it("computeWindowedCanvasSize - returns preferred width based on height and aspect ratio", () => {
    const result = computeWindowedCanvasSize(720, NES_AR, 1);
    const expectedWidth = Math.round(720 * NES_AR);
    expect(result.cssWidth).toBe(`${expectedWidth}px`);
});

it("computeWindowedCanvasSize - cssHeight is 'auto' to allow responsive CSS width clamping", () => {
    // When max-width:100% clamps the canvas width, height:auto lets it scale proportionally.
    // A fixed pixel height would stretch the image vertically when the window is narrow.
    const result = computeWindowedCanvasSize(720, NES_AR, 1);
    expect(result.cssHeight).toBe("auto");
});

it("computeWindowedCanvasSize - pixel dimensions are set for correct canvas rendering", () => {
    const dpr = 2;
    const result = computeWindowedCanvasSize(720, NES_AR, dpr);
    const expectedCssWidth = Math.round(720 * NES_AR);
    expect(result.pixelWidth).toBe(Math.round(expectedCssWidth * dpr));
    expect(result.pixelHeight).toBe(Math.round(720 * dpr));
});

it("computeWindowedCanvasSize - height is clamped to valid NES display range", () => {
    const small = computeWindowedCanvasSize(100, NES_AR, 1);
    expect(small.pixelHeight).toBe(240); // min

    const large = computeWindowedCanvasSize(9999, NES_AR, 1);
    expect(large.pixelHeight).toBe(1440); // max
});

