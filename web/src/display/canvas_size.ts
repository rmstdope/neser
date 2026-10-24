import { MIN_CANVAS_HEIGHT } from "./zoom_controls";

/** The preferred windowed picture height before any zoom, in CSS pixels. */
export const PREFERRED_WINDOWED_HEIGHT = 720;

/**
 * What surrounds the windowed picture vertically: the top bar, the steel frame, the control bar
 * under the picture and the status line, in CSS pixels.
 */
export const WINDOWED_CHROME_HEIGHT = 260;

/**
 * The windowed picture height a page starts with: the preferred 720, or less when the window is too
 * short to show it with the control bar beneath, but never below the NES's 240 lines.
 */
export function initialWindowedHeight(viewportHeight: number): number {
    return Math.max(MIN_CANVAS_HEIGHT, Math.min(PREFERRED_WINDOWED_HEIGHT, Math.floor(viewportHeight - WINDOWED_CHROME_HEIGHT)));
}

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
    const h = Math.max(MIN_CANVAS_HEIGHT, Math.min(preferredHeight, 1440));
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

/**
 * Computes canvas dimensions for a handheld device, maintaining NES aspect ratio.
 *
 * - Portrait: the canvas fills the full viewport width; height is derived from the
 *   aspect ratio and the CSS height is set to "auto" so CSS proportional scaling works.
 * - Landscape: the canvas fills the full viewport height and the width is derived from
 *   the aspect ratio (letterbox-style).
 *
 * @param {boolean} isPortrait     - True when device is in portrait orientation
 * @param {number}  vpWidth        - Viewport CSS width in pixels
 * @param {number}  vpHeight       - Viewport CSS height in pixels
 * @param {number}  nesAspectRatio - NES display aspect ratio (width / height)
 * @param {number}  dpr            - Device pixel ratio
 * @returns {{ cssWidth: string, cssHeight: string, pixelWidth: number, pixelHeight: number }}
 */
export function computeHandheldCanvasSize(isPortrait: boolean, vpWidth: number, vpHeight: number, nesAspectRatio: number, dpr: number) {
    if (isPortrait) {
        const pixelHeight = Math.round(vpWidth / nesAspectRatio);
        return {
            cssWidth: `${vpWidth}px`,
            cssHeight: "auto",
            pixelWidth: vpWidth * dpr,
            pixelHeight: pixelHeight * dpr,
        };
    } else {
        const cssWidth = Math.round(vpHeight * nesAspectRatio);
        return {
            cssWidth: `${cssWidth}px`,
            cssHeight: `${vpHeight}px`,
            pixelWidth: cssWidth * dpr,
            pixelHeight: vpHeight * dpr,
        };
    }
}


/**
 * Sets a canvas's backing-store size, assigning only the dimensions that differ.
 *
 * Every assignment to `width` or `height`, even of the value it already has, reallocates the
 * drawing buffer, which costs seconds on software GL (nr-b5h).
 *
 * @returns whether anything was assigned
 */
export function assignBackingStoreSize(canvas: { width: number; height: number }, pixelWidth: number, pixelHeight: number): boolean {
    let assigned = false;
    if (canvas.width !== pixelWidth) {
        canvas.width = pixelWidth;
        assigned = true;
    }
    if (canvas.height !== pixelHeight) {
        canvas.height = pixelHeight;
        assigned = true;
    }
    return assigned;
}

/**
 * How much of the debugger stays inside the window when it docks under the picture on a narrow
 * window: its title, the registers and the current instruction, in CSS pixels.
 */
export const DOCKED_DEBUGGER_VISIBLE_TOP = 200;

/**
 * The widest the picture may be shown with the debugger docked under it: short enough that the
 * debugger's top 200px stay in the window, but never below the console's own 1x (nr-i5pl).
 *
 * @param viewportHeight - The window's height in CSS pixels
 * @param pictureTop     - The picture's top in document coordinates, so scrolling never moves the cap
 * @param gapToDebugger  - From the picture's bottom to the debugger's top: frame and control bar
 * @param nativeHeight   - The running console's own lines
 * @param aspectRatio    - The picture's width / height
 * @returns the CSS width cap in pixels
 */
export function dockedPictureMaxWidth(viewportHeight: number, pictureTop: number, gapToDebugger: number, nativeHeight: number, aspectRatio: number): number {
    const height = Math.max(nativeHeight, viewportHeight - pictureTop - gapToDebugger - DOCKED_DEBUGGER_VISIBLE_TOP);
    return Math.floor(height * aspectRatio);
}

/**
 * The scroll down that brings a docked debugger's top 200px into the window; 0 when they are in it.
 *
 * @param debuggerTop    - The debugger's top relative to the window, in CSS pixels
 * @param viewportHeight - The window's height in CSS pixels
 */
export function scrollToShowDockedDebugger(debuggerTop: number, viewportHeight: number): number {
    return Math.max(0, debuggerTop + DOCKED_DEBUGGER_VISIBLE_TOP - viewportHeight);
}
