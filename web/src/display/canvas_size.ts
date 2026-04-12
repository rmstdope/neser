/**
 * Computes canvas dimensions for windowed (non-fullscreen) display.
 * CSS height is intentionally "auto" so that when CSS max-width:100% clamps the canvas
 * to a narrow window, the height scales proportionally instead of stretching the image.
 *
 * @param {number} preferredHeight - Desired display height in CSS pixels
 * @param {number} nesAspectRatio  - NES display aspect ratio (width / height)
 * @param {number} dpr             - Device pixel ratio
 * @returns {{ cssWidth: string, cssHeight: string, pixelWidth: number, pixelHeight: number }}
 */
export function computeWindowedCanvasSize(preferredHeight: number, nesAspectRatio: number, dpr: number) {
    const h = Math.max(240, Math.min(preferredHeight, 1440));
    const w = Math.round(h * nesAspectRatio);
    return {
        cssWidth: `${w}px`,
        cssHeight: "auto",
        pixelWidth: Math.round(w * dpr),
        pixelHeight: Math.round(h * dpr),
    };
}

/**
 * Computes canvas dimensions for fullscreen display, maintaining NES aspect ratio.
 *
 * @param {number} availableWidth  - Available CSS width in pixels (viewport minus any padding)
 * @param {number} availableHeight - Available CSS height in pixels
 * @param {number} nesAspectRatio  - NES display aspect ratio (width / height)
 * @param {number} dpr             - Device pixel ratio
 * @returns {{ cssWidth: string, cssHeight: string, pixelWidth: number, pixelHeight: number }}
 */
export function computeFullscreenCanvasSize(availableWidth: number, availableHeight: number, nesAspectRatio: number, dpr: number) {
    const viewportAspect = availableWidth / availableHeight;

    let cssWidth, cssHeight;
    if (viewportAspect > nesAspectRatio) {
        cssHeight = availableHeight;
        cssWidth = Math.round(availableHeight * nesAspectRatio);
    } else {
        cssWidth = availableWidth;
        cssHeight = Math.round(availableWidth / nesAspectRatio);
    }

    return {
        cssWidth: `${cssWidth}px`,
        cssHeight: `${cssHeight}px`,
        pixelWidth: Math.round(cssWidth * dpr),
        pixelHeight: Math.round(cssHeight * dpr),
    };
}

