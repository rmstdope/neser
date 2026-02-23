import test from "node:test";
import assert from "node:assert/strict";
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

test("advanceZoomState disables zoom-in when pressing zoom-in does not increase display height", () => {
    const result = advanceZoomState(createZoomInput());

    assert.equal(result.currentHeight, 720);
    assert.equal(result.plusDisabled, true);
});

test("advanceZoomState disables zoom-out when clamped at minimum height", () => {
    const result = advanceZoomState(createZoomInput({
        direction: "out",
        currentHeight: 240,
        previousDisplayHeight: 240,
        nextDisplayHeight: 240,
    }));

    assert.equal(result.currentHeight, 240);
    assert.equal(result.minusDisabled, true);
});

test("advanceZoomState disables zoom-out when pressing zoom-out does not decrease display height", () => {
    const result = advanceZoomState(createZoomInput({
        direction: "out",
        currentHeight: 720,
        previousDisplayHeight: 700,
        nextDisplayHeight: 700,
    }));

    assert.equal(result.currentHeight, 720);
    assert.equal(result.minusDisabled, true);
    assert.equal(result.plusDisabled, false);
});

test("advanceZoomState advances height when zoom-in increases display height", () => {
    const result = advanceZoomState(createZoomInput({
        nextDisplayHeight: 820,
    }));

    assert.equal(result.currentHeight, 840);
    assert.equal(result.plusDisabled, false);
});

test("nextViewportZoomBlocks keeps opposite direction blocked when current attempt is rejected", () => {
    const result = nextViewportZoomBlocks({
        direction: "in",
        accepted: false,
        currentHeight: 720,
        zoomInBlockedByViewport: false,
        zoomOutBlockedByViewport: true,
    });

    assert.equal(result.zoomInBlockedByViewport, true);
    assert.equal(result.zoomOutBlockedByViewport, true);
});

test("nextViewportZoomBlocks clears both blocks after an accepted zoom step", () => {
    const result = nextViewportZoomBlocks({
        direction: "out",
        accepted: true,
        currentHeight: 600,
        zoomInBlockedByViewport: true,
        zoomOutBlockedByViewport: true,
    });

    assert.equal(result.zoomInBlockedByViewport, false);
    assert.equal(result.zoomOutBlockedByViewport, false);
});

test("findNextVisibleZoomHeight skips no-op zoom-out steps and returns first visible decrease", () => {
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

    assert.equal(next, 360);
});

test("findNextVisibleZoomHeight returns null when no visible zoom-in step exists", () => {
    const next = findNextVisibleZoomHeight({
        direction: "in",
        currentHeight: 720,
        step: 120,
        measureDisplayHeight: () => 400,
    });

    assert.equal(next, null);
});
