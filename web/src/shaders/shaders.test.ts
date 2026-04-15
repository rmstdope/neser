/**
 * Verify that extracted GLSL shader files are importable via Vite's ?raw
 * and contain expected GLSL content markers.
 */
import { describe, it, expect } from "vitest";

import commonVert from "./common.vert.glsl?raw";
import stockFrag from "./stock.frag.glsl?raw";
import crtFrag from "./crt.frag.glsl?raw";
import ntscPass1Vert from "./ntsc-pass1.vert.glsl?raw";
import ntscPass1Frag from "./ntsc-pass1.frag.glsl?raw";
import ntscPass2Vert from "./ntsc-pass2.vert.glsl?raw";
import ntscPass2Frag from "./ntsc-pass2.frag.glsl?raw";
import gbPass0Vert from "./gb-pass0.vert.glsl?raw";
import gbPass0Frag from "./gb-pass0.frag.glsl?raw";
import gbPass1Vert from "./gb-pass1.vert.glsl?raw";
import gbPass1Frag from "./gb-pass1.frag.glsl?raw";
import gbBlurVert from "./gb-blur.vert.glsl?raw";
import gbPass2Frag from "./gb-pass2.frag.glsl?raw";
import gbPass3Frag from "./gb-pass3.frag.glsl?raw";
import gbPass4Vert from "./gb-pass4.vert.glsl?raw";
import gbPass4Frag from "./gb-pass4.frag.glsl?raw";

describe("Extracted GLSL shader files", () => {
    it("common vertex shader contains gl_Position", () => {
        expect(commonVert).toContain("gl_Position");
        expect(commonVert).toContain("a_position");
    });

    it("stock fragment shader contains gl_FragColor passthrough", () => {
        expect(stockFrag).toContain("gl_FragColor");
        expect(stockFrag).toContain("texture2D");
    });

    it("CRT fragment shader contains CRT-specific functions", () => {
        expect(crtFrag).toContain("Warp");
        expect(crtFrag).toContain("Mask");
        expect(crtFrag).toContain("u_hardScan");
    });

    it("NTSC pass 1 shaders contain chroma modulation", () => {
        expect(ntscPass1Vert).toContain("v_pixNo");
        expect(ntscPass1Frag).toContain("CHROMA_MOD_FREQ");
        expect(ntscPass1Frag).toContain("rgb2yiq");
    });

    it("NTSC pass 2 shaders contain FIR filter taps", () => {
        expect(ntscPass2Vert).toContain("u_sourceSize");
        expect(ntscPass2Frag).toContain("lumaTap");
        expect(ntscPass2Frag).toContain("chromaTap");
    });

    it("GB pass 0 shaders contain dot-matrix pattern", () => {
        expect(gbPass0Vert).toContain("v_dotSizeInPx");
        expect(gbPass0Frag).toContain("intersect_rect");
    });

    it("GB pass 1 shaders contain blending", () => {
        expect(gbPass1Vert).toContain("v_blurUp");
        expect(gbPass1Frag).toContain("ADJACENT_BLEND");
    });

    it("GB blur vertex shader contains texel setup", () => {
        expect(gbBlurVert).toContain("v_texel");
        expect(gbBlurVert).toContain("v_lowerBound");
    });

    it("GB pass 2 fragment contains horizontal blur weights", () => {
        expect(gbPass2Frag).toContain("v_texel.x");
        expect(gbPass2Frag).toContain("0.13465834");
    });

    it("GB pass 3 fragment contains vertical blur weights", () => {
        expect(gbPass3Frag).toContain("v_texel.y");
        expect(gbPass3Frag).toContain("0.13465834");
    });

    it("GB pass 4 shaders contain compositing", () => {
        expect(gbPass4Vert).toContain("v_shadowScaleFactor");
        expect(gbPass4Frag).toContain("SHADOW_OPACITY");
        expect(gbPass4Frag).toContain("u_background");
    });

    it("all 16 shader sources are non-empty strings", () => {
        const all = [
            commonVert, stockFrag, crtFrag,
            ntscPass1Vert, ntscPass1Frag, ntscPass2Vert, ntscPass2Frag,
            gbPass0Vert, gbPass0Frag, gbPass1Vert, gbPass1Frag,
            gbBlurVert, gbPass2Frag, gbPass3Frag,
            gbPass4Vert, gbPass4Frag,
        ];
        for (const src of all) {
            expect(typeof src).toBe("string");
            expect(src.length).toBeGreaterThan(50);
        }
    });
});
