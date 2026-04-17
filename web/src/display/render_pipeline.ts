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
