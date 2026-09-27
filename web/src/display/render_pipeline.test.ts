import { describe, expect, it } from "vitest";

import { SCREEN_CONTEXT_ATTRIBUTES, selectRenderPipeline } from "./render_pipeline";

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

describe("SCREEN_CONTEXT_ATTRIBUTES", () => {
    // Every screen draw is one full-viewport quad, so multisampling changes no visible pixel, while
    // it costs synchronous GPU round trips on every canvas resize (nr-v5x).
    it("requests the screen context without antialiasing", () => {
        expect(SCREEN_CONTEXT_ATTRIBUTES.antialias).toBe(false);
    });
});
