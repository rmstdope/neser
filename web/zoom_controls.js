export const MIN_CANVAS_HEIGHT = 240;
export const MAX_CANVAS_HEIGHT = 1440;

export function clampCanvasHeight(height, minHeight = MIN_CANVAS_HEIGHT, maxHeight = MAX_CANVAS_HEIGHT) {
    return Math.max(minHeight, Math.min(height, maxHeight));
}

export function findNextVisibleZoomHeight({
    direction,
    currentHeight,
    step,
    measureDisplayHeight,
    minHeight = MIN_CANVAS_HEIGHT,
    maxHeight = MAX_CANVAS_HEIGHT,
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

function didDisplayMoveInDirection(direction, previousDisplayHeight, nextDisplayHeight) {
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
