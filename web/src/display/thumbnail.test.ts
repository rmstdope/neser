/**
 * @vitest-environment jsdom
 */
import { afterEach, describe, expect, it, vi } from "vitest";

import { frameThumbnail, frameToRgba } from "./thumbnail";

describe("frameToRgba", () => {
    it("Given an RGBA frame, When converted, Then every pixel is kept and made opaque", () => {
        const out = frameToRgba(new Uint8Array([1, 2, 3, 0, 4, 5, 6, 9]), 4, 2, 1);
        expect([...out]).toEqual([1, 2, 3, 255, 4, 5, 6, 255]);
    });

    it("Given an RGB frame (the GBA's), When converted, Then each pixel gains an opaque alpha", () => {
        const out = frameToRgba(new Uint8Array([1, 2, 3, 4, 5, 6]), 3, 2, 1);
        expect([...out]).toEqual([1, 2, 3, 255, 4, 5, 6, 255]);
    });

    it("Given a frame shorter than its size, When converted, Then the missing pixels are black", () => {
        const out = frameToRgba(new Uint8Array([1, 2, 3]), 3, 2, 1);
        expect([...out]).toEqual([1, 2, 3, 255, 0, 0, 0, 255]);
    });
});

describe("frameThumbnail", () => {
    afterEach(() => vi.restoreAllMocks());

    it("Given a running game, When its picture is taken, Then both canvases are CPU-backed so taking it never waits on the busy GPU", () => {
        const asked: unknown[] = [];
        vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockImplementation(function (this: HTMLCanvasElement, _kind: string, options?: unknown) {
            asked.push(options);
            return { putImageData() {}, drawImage() {}, imageSmoothingEnabled: true } as unknown as CanvasRenderingContext2D;
        } as typeof HTMLCanvasElement.prototype.getContext);
        vi.spyOn(HTMLCanvasElement.prototype, "toDataURL").mockReturnValue("data:image/png;base64,AAA");
        vi.stubGlobal("ImageData", class { constructor(public data: Uint8ClampedArray, public width: number, public height: number) {} });

        expect(frameThumbnail(new Uint8Array(4 * 4 * 4), 4, 4, 4, 2)).toBe("data:image/png;base64,AAA");
        expect(asked).toEqual([{ willReadFrequently: true }, { willReadFrequently: true }]);
        vi.unstubAllGlobals();
    });
});
