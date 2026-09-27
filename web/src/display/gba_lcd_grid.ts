/**
 * Geometry of the web LCD Grid look (gba-lcd-grid-v2): how large to draw the GBA console art and
 * the picture inside it for a given canvas.
 *
 * Desktop runs the preset with its fixed `video_scale` of 4, so the art is drawn at a fixed pixel
 * size. On the web the console scales with the picture instead: the scale is chosen so the whole
 * console body fits the canvas, and it is recomputed whenever the canvas changes size.
 */

/** The border art (resources/gba-border-square-4x.png) at 1x: the picture's 240x160 hole is centred in it. */
export const BORDER_ART_1X = { width: 800, height: 400 } as const;

/** The console body inside the art at 1x, measured from the image (opaque, not near-black). */
export const CONSOLE_BODY_1X = { left: 126, right: 674, top: 60, bottom: 372 } as const;

/** Room kept around the body, in 1x art pixels, on the side that limits the scale. */
const MARGIN_1X = 6;

/**
 * The `video_scale` (canvas pixels per GBA pixel) at which the console body fits the canvas,
 * centred on the picture.
 */
export function lcdGridVideoScale(canvasWidth: number, canvasHeight: number): number {
    const cx = BORDER_ART_1X.width / 2;
    const cy = BORDER_ART_1X.height / 2;
    const halfWidth = Math.max(cx - CONSOLE_BODY_1X.left, CONSOLE_BODY_1X.right - cx) + MARGIN_1X;
    const halfHeight = Math.max(cy - CONSOLE_BODY_1X.top, CONSOLE_BODY_1X.bottom - cy) + MARGIN_1X;
    return Math.min(canvasWidth / (2 * halfWidth), canvasHeight / (2 * halfHeight));
}
