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
import gbaPassVert from "./gba-pass.vert.glsl?raw";
import gbaAgb001Frag from "./gba-agb001.frag.glsl?raw";
import gbaNsoColorFrag from "./gba-nso-color.frag.glsl?raw";
import gbaMultiLutFrag from "./gba-multilut.frag.glsl?raw";
import gbaSp101ColorFrag from "./gba-sp101-color.frag.glsl?raw";
import gbaResponseTimeFrag from "./gba-response-time.frag.glsl?raw";
import gbaLcdGridFrag from "./gba-lcd-grid.frag.glsl?raw";
import gbaColorFrag from "./gba-color.frag.glsl?raw";
import gbaBorderFrag from "./gba-border.frag.glsl?raw";

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

    it("the GBA pass vertex shader flips only the pass drawn to the screen", () => {
        expect(gbaPassVert).toContain("u_flipY");
    });

    it("AGB-001 keeps mGBA's 4x4 sub-pixel pattern and the page opaque", () => {
        expect(gbaAgb001Frag).toContain("vec3(1.0, 0.2, 0.2)");
        expect(gbaAgb001Frag).toContain("vec3(0.8)");
        expect(gbaAgb001Frag).toContain("gl_FragColor = vec4(color * arrayX * arrayY, 1.0);");
    });

    it("Switch Online uses the sRGB profile with darken_screen 0.8", () => {
        expect(gbaNsoColorFrag).toContain("0.865, 0.0575, 0.0575");
        expect(gbaNsoColorFrag).toContain("DARKEN_SCREEN 0.8");
    });

    it("GBA SP runs the 64-entry LUT and then the SP-101 sRGB profile", () => {
        expect(gbaMultiLutFrag).toContain("LUT_SIZE 64.0");
        expect(gbaMultiLutFrag).toContain("u_lut");
        expect(gbaSp101ColorFrag).toContain("0.96, 0.0325, 0.001");
        expect(gbaSp101ColorFrag).toContain("LUM 0.935");
    });

    it("LCD Grid runs response time, the cgwg grid, GBA colour and the console border", () => {
        expect(gbaResponseTimeFrag).toContain("RESPONSE_TIME 0.333");
        expect(gbaResponseTimeFrag).toContain("u_history7");
        expect(gbaLcdGridFrag).toContain("LCD_GAMMA 2.6");
        expect(gbaLcdGridFrag).toContain("u_videoScale");
        expect(gbaColorFrag).toContain("0.905, 0.10, 0.1575");
        expect(gbaBorderFrag).toContain("u_border");
        expect(gbaBorderFrag).toContain("vec2(800.0, 400.0) * u_videoScale");
    });

    it("every shader source are non-empty strings", () => {
        const all = [
            commonVert, stockFrag, crtFrag,
            ntscPass1Vert, ntscPass1Frag, ntscPass2Vert, ntscPass2Frag,
            gbPass0Vert, gbPass0Frag, gbPass1Vert, gbPass1Frag,
            gbBlurVert, gbPass2Frag, gbPass3Frag,
            gbPass4Vert, gbPass4Frag,
            gbaPassVert, gbaAgb001Frag, gbaNsoColorFrag, gbaMultiLutFrag, gbaSp101ColorFrag,
            gbaResponseTimeFrag, gbaLcdGridFrag, gbaColorFrag, gbaBorderFrag,
        ];
        for (const src of all) {
            expect(typeof src).toBe("string");
            expect(src.length).toBeGreaterThan(50);
        }
    });
});
