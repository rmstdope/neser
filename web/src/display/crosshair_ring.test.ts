import { afterEach, expect, it } from "vitest";
import { createCrosshair } from "./crosshair";

type Stroke = { color: string; width: number; arcs: number[] };

function mockCanvas(width: number) {
    const strokes: Stroke[] = [];
    let arcs: number[] = [];
    const ctx = {
        clearRect: () => {},
        strokeStyle: "",
        fillStyle: "",
        lineWidth: 0,
        lineCap: "",
        beginPath: () => {},
        moveTo: () => {},
        lineTo: () => {},
        arc: (_x: number, _y: number, r: number) => {
            arcs.push(r);
        },
        stroke() {
            strokes.push({ color: ctx.strokeStyle, width: ctx.lineWidth, arcs });
            arcs = [];
        },
        fill: () => {},
    };
    const canvas = {
        width,
        height: (width * 224) / 256,
        offsetLeft: 0,
        offsetTop: 0,
        style: { width: "", height: "" } as Record<string, string>,
        getContext: () => ctx,
        remove: () => {},
        parentElement: { style: { position: "" }, appendChild: () => {} },
    };
    return { canvas, strokes };
}

const originalDocument = globalThis.document;
const originalWindow = globalThis.window;

afterEach(() => {
    globalThis.document = originalDocument;
    globalThis.window = originalWindow;
});

it("draws the Super Scope ring, black under white, scaled with the picture", () => {
    const overlay = mockCanvas(512);
    globalThis.document = { createElement: () => overlay.canvas } as any;
    globalThis.window = { devicePixelRatio: 1 } as any;
    const target = mockCanvas(512);

    const sight = createCrosshair(target.canvas as unknown as HTMLCanvasElement, { style: "ring" });
    sight.show();
    sight.updatePosition(100, 100);

    const last = overlay.strokes.slice(-4);
    // Ring and ticks in black (3.5 picture pixels) then white (1.5), at twice native size.
    expect(last.map((s) => [s.color, s.width, s.arcs])).toEqual([
        ["#000", 7, [18]],
        ["#000", 7, []],
        ["#fff", 3, [18]],
        ["#fff", 3, []],
    ]);
});
