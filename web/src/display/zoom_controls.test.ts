import { expect, it } from "vitest";
import {
    advanceZoomState,
    findNextVisibleZoomHeight,
    nextViewportZoomBlocks,
    probeNextZoomHeight,
    probeZoomAvailability,
} from "./zoom_controls";

function createZoomInput(overrides: Partial<{ direction: "in" | "out"; currentHeight: number; step: number; previousDisplayHeight: number; nextDisplayHeight: number }> = {}) {
    return {
        direction: "in" as const,
        currentHeight: 720,
        step: 120,
        previousDisplayHeight: 700,
        nextDisplayHeight: 700,
        ...overrides,
    };
}

it("advanceZoomState disables zoom-in when pressing zoom-in does not increase display height", () => {
    const result = advanceZoomState(createZoomInput());

    expect(result.currentHeight).toBe(720);
    expect(result.plusDisabled).toBe(true);
});

it("advanceZoomState disables zoom-out when clamped at minimum height", () => {
    const result = advanceZoomState(createZoomInput({
        direction: "out",
        currentHeight: 240,
        previousDisplayHeight: 240,
        nextDisplayHeight: 240,
    }));

    expect(result.currentHeight).toBe(240);
    expect(result.minusDisabled).toBe(true);
});

it("advanceZoomState disables zoom-out when pressing zoom-out does not decrease display height", () => {
    const result = advanceZoomState(createZoomInput({
        direction: "out",
        currentHeight: 720,
        previousDisplayHeight: 700,
        nextDisplayHeight: 700,
    }));

    expect(result.currentHeight).toBe(720);
    expect(result.minusDisabled).toBe(true);
    expect(result.plusDisabled).toBe(false);
});

it("advanceZoomState advances height when zoom-in increases display height", () => {
    const result = advanceZoomState(createZoomInput({
        nextDisplayHeight: 820,
    }));

    expect(result.currentHeight).toBe(840);
    expect(result.plusDisabled).toBe(false);
});

it("nextViewportZoomBlocks keeps opposite direction blocked when current attempt is rejected", () => {
    const result = nextViewportZoomBlocks({
        direction: "in",
        accepted: false,
        currentHeight: 720,
        zoomInBlockedByViewport: false,
        zoomOutBlockedByViewport: true,
    });

    expect(result.zoomInBlockedByViewport).toBe(true);
    expect(result.zoomOutBlockedByViewport).toBe(true);
});

it("nextViewportZoomBlocks clears both blocks after an accepted zoom step", () => {
    const result = nextViewportZoomBlocks({
        direction: "out",
        accepted: true,
        currentHeight: 600,
        zoomInBlockedByViewport: true,
        zoomOutBlockedByViewport: true,
    });

    expect(result.zoomInBlockedByViewport).toBe(false);
    expect(result.zoomOutBlockedByViewport).toBe(false);
});

it("findNextVisibleZoomHeight skips no-op zoom-out steps and returns first visible decrease", () => {
    const displayByHeight = new Map([
        [720, 400],
        [600, 400],
        [480, 400],
        [360, 320],
        [240, 240],
    ]);

    const next = findNextVisibleZoomHeight({
        direction: "out",
        currentHeight: 720,
        step: 120,
        measureDisplayHeight: (height: number) => displayByHeight.get(height)!,
    });

    expect(next).toBe(360);
});

it("findNextVisibleZoomHeight returns null when no visible zoom-in step exists", () => {
    const next = findNextVisibleZoomHeight({
        direction: "in",
        currentHeight: 720,
        step: 120,
        measureDisplayHeight: () => 400,
    });

    expect(next).toBe(null);
});

const ASPECT = 256 / 240;
const cssWidthFor = (height: number) => `${Math.round(height * ASPECT)}px`;

/**
 * A windowed canvas as the page lays it out: `height: auto` and `max-width: 100%` of a container
 * `containerWidth` wide, so its displayed height is its CSS width at the backing store's ratio. Writes are
 * recorded, since each reallocates the GL drawing buffer.
 */
function layoutCanvas(startHeight: number, containerWidth: number) {
    const backingStoreWrites: string[] = [];
    const startWidth = cssWidthFor(startHeight);
    return {
        backingStoreWrites,
        style: { width: startWidth },
        get clientHeight() {
            // `height: auto` takes the intrinsic ratio of the backing store.
            const displayedWidth = Math.min(parseFloat(this.style.width), containerWidth);
            return Math.round(displayedWidth * this.height / this.width);
        },
        get width() { return Math.round(parseFloat(startWidth)); },
        set width(value: number) { backingStoreWrites.push(`width=${value}`); },
        get height() { return startHeight; },
        set height(value: number) { backingStoreWrites.push(`height=${value}`); },
    };
}

it("probeNextZoomHeight returns the next step up without touching the backing store", () => {
    const canvas = layoutCanvas(720, 4000);
    const next = probeNextZoomHeight({ canvas, direction: "in", currentHeight: 720, step: 120, cssWidthFor });

    expect(next).toBe(840);
    expect(canvas.backingStoreWrites).toEqual([]);
    expect(canvas.style.width).toBe(cssWidthFor(720));
});

it("probeNextZoomHeight skips steps the container clamps away and returns the first visible decrease", () => {
    // A container 384 px wide shows every height from 360 up at 360 px.
    const canvas = layoutCanvas(720, 384);
    const next = probeNextZoomHeight({ canvas, direction: "out", currentHeight: 720, step: 120, cssWidthFor });

    expect(next).toBe(240);
    expect(canvas.backingStoreWrites).toEqual([]);
    expect(canvas.style.width).toBe(cssWidthFor(720));
});

it("probeNextZoomHeight returns null when no step changes the displayed height, and restores the CSS width", () => {
    const canvas = layoutCanvas(720, 384);
    const next = probeNextZoomHeight({ canvas, direction: "in", currentHeight: 720, step: 120, cssWidthFor });

    expect(next).toBe(null);
    expect(canvas.backingStoreWrites).toEqual([]);
    expect(canvas.style.width).toBe(cssWidthFor(720));
});

it("probeZoomAvailability reports both directions without touching the backing store", () => {
    const roomy = layoutCanvas(720, 4000);
    expect(probeZoomAvailability({ canvas: roomy, currentHeight: 720, step: 120, cssWidthFor }))
        .toEqual({ canZoomIn: true, canZoomOut: true });

    const clamped = layoutCanvas(720, 384);
    expect(probeZoomAvailability({ canvas: clamped, currentHeight: 720, step: 120, cssWidthFor }))
        .toEqual({ canZoomIn: false, canZoomOut: true });

    const atMinimum = layoutCanvas(240, 4000);
    expect(probeZoomAvailability({ canvas: atMinimum, currentHeight: 240, step: 120, cssWidthFor }))
        .toEqual({ canZoomIn: true, canZoomOut: false });

    for (const canvas of [roomy, clamped, atMinimum]) {
        expect(canvas.backingStoreWrites).toEqual([]);
    }
    expect(clamped.style.width).toBe(cssWidthFor(720));
});
