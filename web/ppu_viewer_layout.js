const NTSC_PIXEL_ASPECT_WIDTH = 8;
const NTSC_PIXEL_ASPECT_HEIGHT = 7;

const NAMETABLE_SPACE_WIDTH = 512;
const NAMETABLE_SPACE_HEIGHT = 480;
const VISIBLE_WIDTH = 256;
const VISIBLE_HEIGHT = 240;

export function computeNtscDisplayWidth(sourceWidth) {
    if (!Number.isFinite(sourceWidth) || sourceWidth <= 0) {
        return 0;
    }
    return Math.round((sourceWidth * NTSC_PIXEL_ASPECT_WIDTH) / NTSC_PIXEL_ASPECT_HEIGHT);
}

export function computeScrollViewportRects(scrollX, scrollY) {
    const x = ((scrollX % NAMETABLE_SPACE_WIDTH) + NAMETABLE_SPACE_WIDTH) % NAMETABLE_SPACE_WIDTH;
    const y = ((scrollY % NAMETABLE_SPACE_HEIGHT) + NAMETABLE_SPACE_HEIGHT) % NAMETABLE_SPACE_HEIGHT;

    const xWraps = x + VISIBLE_WIDTH > NAMETABLE_SPACE_WIDTH;
    const yWraps = y + VISIBLE_HEIGHT > NAMETABLE_SPACE_HEIGHT;

    const xSegments = xWraps
        ? [
            { x: x, width: NAMETABLE_SPACE_WIDTH - x },
            { x: 0, width: VISIBLE_WIDTH - (NAMETABLE_SPACE_WIDTH - x) },
        ]
        : [{ x: x, width: VISIBLE_WIDTH }];

    const ySegments = yWraps
        ? [
            { y: y, height: NAMETABLE_SPACE_HEIGHT - y },
            { y: 0, height: VISIBLE_HEIGHT - (NAMETABLE_SPACE_HEIGHT - y) },
        ]
        : [{ y: y, height: VISIBLE_HEIGHT }];

    const rects = [];
    for (const xs of xSegments) {
        for (const ys of ySegments) {
            rects.push({
                x: xs.x,
                y: ys.y,
                width: xs.width,
                height: ys.height,
            });
        }
    }
    return rects;
}
