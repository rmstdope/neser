import { describe, expect, it } from "vitest";

import { selectRenderPipeline } from "./render_pipeline";

describe("selectRenderPipeline", () => {
    it("uses the GB pipeline once GB assets are ready", () => {
        expect(
            selectRenderPipeline({
                filterType: "gb",
                gbAssetsLoaded: true,
                hasSinglePassShader: false,
            }),
        ).toBe("gb");
    });

    it("keeps GB rendering off the nullable single-pass shader while assets are loading", () => {
        expect(
            selectRenderPipeline({
                filterType: "gb",
                gbAssetsLoaded: false,
                hasSinglePassShader: false,
            }),
        ).toBe("gb");
    });

    it("returns to single-pass rendering after switching back to NES", () => {
        expect(
            selectRenderPipeline({
                filterType: "single",
                gbAssetsLoaded: false,
                hasSinglePassShader: true,
            }),
        ).toBe("single");
    });

    it("uses the NTSC pipeline for NTSC filters", () => {
        expect(
            selectRenderPipeline({
                filterType: "ntsc",
                gbAssetsLoaded: false,
                hasSinglePassShader: false,
            }),
        ).toBe("ntsc");
    });
});