export type RenderPipeline = "single" | "ntsc" | "gb";

export interface RenderPipelineState {
    filterType: string | undefined;
    gbAssetsLoaded: boolean;
    hasSinglePassShader: boolean;
}

export function selectRenderPipeline(state: RenderPipelineState): RenderPipeline {
    if (state.filterType === "ntsc") {
        return "ntsc";
    }

    if (state.filterType === "gb") {
        if (!state.gbAssetsLoaded) {
            return state.hasSinglePassShader ? "single" : "gb";
        }
        return "gb";
    }

    return "single";
}

/**
 * Context attributes for the screen's WebGL context. Every screen draw is one full-viewport quad,
 * so multisampling changes no visible pixel (at most float rounding on the quad's diagonal), yet
 * it costs a resolve every frame and synchronous GPU round trips whenever the canvas is resized:
 * seconds per zoom click on software GL (nr-v5x).
 */
export const SCREEN_CONTEXT_ATTRIBUTES: Readonly<WebGLContextAttributes> = { antialias: false };
