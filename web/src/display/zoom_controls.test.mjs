import { expect, it } from "vitest";
import {
    advanceZoomState,
    findNextVisibleZoomHeight,
    nextViewportZoomBlocks,
} from "./zoom_controls.js";

function createZoomInput(overrides = {}) {
    return {
        direction: "in",
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
        measureDisplayHeight: (height) => displayByHeight.get(height),
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
