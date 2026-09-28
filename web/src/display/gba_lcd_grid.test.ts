import { describe, it, expect } from "vitest";
import { BORDER_ART_1X, CONSOLE_BODY_1X, lcdGridVideoScale } from "./gba_lcd_grid";

/** Whether the console body, centred on the picture, lies inside the canvas at this scale. */
function bodyFits(scale: number, w: number, h: number) {
    const halfW = (CONSOLE_BODY_1X.right - BORDER_ART_1X.width / 2) * scale;
    const left = (BORDER_ART_1X.width / 2 - CONSOLE_BODY_1X.left) * scale;
    const down = (CONSOLE_BODY_1X.bottom - BORDER_ART_1X.height / 2) * scale;
    const up = (BORDER_ART_1X.height / 2 - CONSOLE_BODY_1X.top) * scale;
    return halfW <= w / 2 && left <= w / 2 && down <= h / 2 && up <= h / 2;
}

describe("LCD Grid scale on the web", () => {
    for (const [label, w, h] of [
        ["windowed 3:2", 720, 480],
        ["hi-dpi windowed", 1440, 960],
        ["wide fullscreen", 2560, 1440],
        ["phone, 3:2 at DPR 1", 390, 260],
    ] as const) {
        it(`fits the whole console around a smaller picture (${label})`, () => {
            const s = lcdGridVideoScale(w, h);
            expect(bodyFits(s, w, h)).toBe(true);
            expect(240 * s).toBeLessThan(w);
            expect(160 * s).toBeLessThan(h);
        });
    }

    it("grows with the canvas, so Fullscreen draws a bigger console", () => {
        expect(lcdGridVideoScale(1440, 960)).toBeCloseTo(2 * lcdGridVideoScale(720, 480));
    });

    it("uses most of the canvas: the body is within a few percent of an edge", () => {
        const s = lcdGridVideoScale(720, 480);
        expect(bodyFits(s * 1.05, 720, 480)).toBe(false);
    });
});
