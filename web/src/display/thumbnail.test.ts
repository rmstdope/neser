import { describe, expect, it } from "vitest";

import { frameToRgba } from "./thumbnail";

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
