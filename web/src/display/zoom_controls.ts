export const MIN_CANVAS_HEIGHT = 240;
export const MAX_CANVAS_HEIGHT = 1440;

export function clampCanvasHeight(height: number, minHeight = MIN_CANVAS_HEIGHT, maxHeight = MAX_CANVAS_HEIGHT) {
    return Math.max(minHeight, Math.min(height, maxHeight));
}

export function findNextVisibleZoomHeight({
    direction,
    currentHeight,
    step,
    measureDisplayHeight,
    minHeight = MIN_CANVAS_HEIGHT,
    maxHeight = MAX_CANVAS_HEIGHT,
}: {
    direction: "in" | "out";
    currentHeight: number;
    step: number;
    measureDisplayHeight: (h: number) => number;
    minHeight?: number;
    maxHeight?: number;
}) {
    const delta = direction === "in" ? step : -step;
    const startDisplayHeight = measureDisplayHeight(currentHeight);
    let probeHeight = currentHeight;

    for (;;) {
        const trialHeight = clampCanvasHeight(probeHeight + delta, minHeight, maxHeight);
        if (trialHeight === probeHeight) {
            return null;
        }

        const trialDisplayHeight = measureDisplayHeight(trialHeight);
        const movedInDirection = direction === "in"
            ? trialDisplayHeight > startDisplayHeight
            : trialDisplayHeight < startDisplayHeight;

        if (movedInDirection) {
            return trialHeight;
        }

        probeHeight = trialHeight;
    }
}

/** The part of a canvas a zoom probe uses: its CSS width, and the height it is displayed at. */
export interface ZoomProbeCanvas {
    style: { width: string };
    readonly clientHeight: number;
}

/**
 * Finds the next zoom height that visibly changes the displayed height, by trying each candidate's
 * CSS width only. The windowed canvas has `height: auto`, so its displayed height follows its CSS
 * width at the backing store's aspect ratio, and the backing store (whose every assignment
 * reallocates the GL drawing buffer) is never touched. The CSS width is restored before returning.
 */
export function probeNextZoomHeight({
    canvas,
    direction,
    currentHeight,
    step,
    cssWidthFor,
}: {
    canvas: ZoomProbeCanvas;
    direction: "in" | "out";
    currentHeight: number;
    step: number;
    cssWidthFor: (height: number) => string;
}): number | null {
    const originalWidth = canvas.style.width;
    try {
        return findNextVisibleZoomHeight({
            direction,
            currentHeight,
            step,
            measureDisplayHeight: (height) => {
                canvas.style.width = cssWidthFor(height);
                return canvas.clientHeight;
            },
        });
    } finally {
        canvas.style.width = originalWidth;
    }
}

/** Whether a zoom step in each direction would visibly change the displayed height. */
export function probeZoomAvailability(args: {
    canvas: ZoomProbeCanvas;
    currentHeight: number;
    step: number;
    cssWidthFor: (height: number) => string;
}): { canZoomIn: boolean; canZoomOut: boolean } {
    return {
        canZoomIn: probeNextZoomHeight({ ...args, direction: "in" }) !== null,
        canZoomOut: probeNextZoomHeight({ ...args, direction: "out" }) !== null,
    };
}

function didDisplayMoveInDirection(direction: "in" | "out", previousDisplayHeight: number, nextDisplayHeight: number) {
    return direction === "in"
        ? nextDisplayHeight > previousDisplayHeight
        : nextDisplayHeight < previousDisplayHeight;
}

export function advanceZoomState({
    direction,
    currentHeight,
    step,
    previousDisplayHeight,
    nextDisplayHeight,
    minHeight = MIN_CANVAS_HEIGHT,
    maxHeight = MAX_CANVAS_HEIGHT,
}: {
    direction: "in" | "out";
    currentHeight: number;
    step: number;
    previousDisplayHeight: number;
    nextDisplayHeight: number;
    minHeight?: number;
    maxHeight?: number;
}) {
    const delta = direction === "in" ? step : -step;
    const clampedHeight = clampCanvasHeight(currentHeight + delta, minHeight, maxHeight);
    const heightChanged = clampedHeight !== currentHeight;
    const displayMovedInDirection = didDisplayMoveInDirection(direction, previousDisplayHeight, nextDisplayHeight);
    const canAcceptZoomStep = heightChanged && displayMovedInDirection;
    const resolvedHeight = canAcceptZoomStep ? clampedHeight : currentHeight;
    const directionBlocked = !canAcceptZoomStep;

    return {
        currentHeight: resolvedHeight,
        plusDisabled: resolvedHeight >= maxHeight || (direction === "in" && directionBlocked),
        minusDisabled: resolvedHeight <= minHeight || (direction === "out" && directionBlocked),
    };
}

export function nextViewportZoomBlocks({
    direction,
    accepted,
    currentHeight,
    zoomInBlockedByViewport,
    zoomOutBlockedByViewport,
    minHeight = MIN_CANVAS_HEIGHT,
    maxHeight = MAX_CANVAS_HEIGHT,
}: {
    direction: "in" | "out";
    accepted: boolean;
    currentHeight: number;
    zoomInBlockedByViewport: boolean;
    zoomOutBlockedByViewport: boolean;
    minHeight?: number;
    maxHeight?: number;
}) {
    if (accepted) {
        return {
            zoomInBlockedByViewport: false,
            zoomOutBlockedByViewport: false,
        };
    }

    if (direction === "in") {
        return {
            zoomInBlockedByViewport: currentHeight < maxHeight,
            zoomOutBlockedByViewport,
        };
    }

    return {
        zoomInBlockedByViewport,
        zoomOutBlockedByViewport: currentHeight > minHeight,
    };
}
