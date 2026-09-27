/**
 * The Game Boy Advance looks on the web: WebGL 1 ports of the desktop slang presets.
 *
 * - AGB-001 (`agb001`): handheld/agb001.slangp — mGBA's AGB-001 pass at 4x, then stock, linear.
 * - Switch Online (`nsoGbaColor`): handheld/color-mod/NSO-gba-color.slangp — one colour pass.
 * - GBA SP (`sp101Color`): handheld/color-mod/sp101-color.slangp — multiLUT (LUT 1), then SP-101 colour.
 * - LCD Grid (`gbaLcdGrid`): handheld/console-border/gba-lcd-grid-v2.slangp — response time over the
 *   last seven frames, cgwg's LCD grid at the viewport size, GBA colour, then the console border.
 *
 * Every pass keeps the picture's top row at t = 0 and only the pass drawn to the screen flips
 * (see gba-pass.vert.glsl). Programs and fetched images are made the first time a look is
 * prepared and kept for the page; the canvas-sized targets follow the canvas.
 */

import { lcdGridVideoScale } from "./gba_lcd_grid";
import passVert from "../shaders/gba-pass.vert.glsl?raw";
import stockFrag from "../shaders/stock.frag.glsl?raw";
import agb001Frag from "../shaders/gba-agb001.frag.glsl?raw";
import nsoColorFrag from "../shaders/gba-nso-color.frag.glsl?raw";
import multiLutFrag from "../shaders/gba-multilut.frag.glsl?raw";
import sp101ColorFrag from "../shaders/gba-sp101-color.frag.glsl?raw";
import responseTimeFrag from "../shaders/gba-response-time.frag.glsl?raw";
import lcdGridFrag from "../shaders/gba-lcd-grid.frag.glsl?raw";
import gbaColorFrag from "../shaders/gba-color.frag.glsl?raw";
import borderFrag from "../shaders/gba-border.frag.glsl?raw";

export const GBA_LOOKS = ["agb001", "nsoGbaColor", "sp101Color", "gbaLcdGrid"] as const;
export type GbaLook = (typeof GBA_LOOKS)[number];

export function isGbaLook(key: string): key is GbaLook {
    return (GBA_LOOKS as readonly string[]).includes(key);
}

export interface GbaPipelineDeps {
    /** Compiles and links a program, or returns null (and logs) on failure. */
    createProgram(vertexSource: string, fragmentSource: string): WebGLProgram | null;
    /** Fetches an image into a texture; resolves null when it cannot be fetched. */
    loadImageTexture(url: string, linear: boolean): Promise<WebGLTexture | null>;
    /** The full-screen quad's positions, (-1,-1) to (1,1) as a triangle strip. */
    positionBuffer(): WebGLBuffer | null;
}

export interface GbaPipeline {
    /** Compiles the look's programs and fetches its images the first time; false if either fails. */
    prepare(look: GbaLook): Promise<boolean>;
    /** Draws one frame with a prepared look to the canvas. */
    render(look: GbaLook, frame: Uint8Array, format: number, srcW: number, srcH: number, outW: number, outH: number): boolean;
}

interface Target {
    tex: WebGLTexture;
    fbo: WebGLFramebuffer;
    w: number;
    h: number;
}

const HISTORY = 7;

export function createGbaPipeline(gl: WebGLRenderingContext, deps: GbaPipelineDeps): GbaPipeline {
    const programs = new Map<string, WebGLProgram>();
    const targets = new Map<string, Target>();
    const images = new Map<string, WebGLTexture>();
    const pending = new Map<GbaLook, Promise<boolean>>();
    /** frames[0] is the current frame; frames[k] the frame k frames ago. */
    let frames: WebGLTexture[] = [];
    let historyFilled = false;

    function program(name: string, frag: string): WebGLProgram | null {
        const existing = programs.get(name);
        if (existing) return existing;
        const made = deps.createProgram(passVert, frag);
        if (made) programs.set(name, made);
        return made;
    }

    const lookPrograms: Record<GbaLook, [string, string][]> = {
        agb001: [["agb001", agb001Frag], ["stock", stockFrag]],
        nsoGbaColor: [["nso", nsoColorFrag]],
        sp101Color: [["multiLut", multiLutFrag], ["sp101", sp101ColorFrag]],
        gbaLcdGrid: [["responseTime", responseTimeFrag], ["lcdGrid", lcdGridFrag], ["gbaColor", gbaColorFrag], ["border", borderFrag]],
    };
    const lookImages: Partial<Record<GbaLook, [string, string]>> = {
        sp101Color: ["lut", new URL("../assets/gba-lut-64.png", import.meta.url).href],
        gbaLcdGrid: ["border", new URL("../assets/gba-border-square-4x.png", import.meta.url).href],
    };

    function prepare(look: GbaLook): Promise<boolean> {
        const inFlight = pending.get(look);
        if (inFlight) return inFlight;
        for (const [name, frag] of lookPrograms[look]) {
            if (!program(name, frag)) return Promise.resolve(false);
        }
        const image = lookImages[look];
        if (!image || images.has(image[0])) return Promise.resolve(true);
        const [name, url] = image;
        const loading = deps.loadImageTexture(url, true).then((tex) => {
            pending.delete(look);
            if (!tex) return false;
            images.set(name, tex);
            return true;
        });
        pending.set(look, loading);
        return loading;
    }

    function makeTexture(w: number, h: number, linear: boolean): WebGLTexture {
        const tex = gl.createTexture()!;
        gl.bindTexture(gl.TEXTURE_2D, tex);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
        const filter = linear ? gl.LINEAR : gl.NEAREST;
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, filter);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, filter);
        gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, w, h, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
        return tex;
    }

    /** A render target of the given size, remade when the size changes. */
    function target(name: string, w: number, h: number, linear: boolean): Target | null {
        const existing = targets.get(name);
        if (existing && existing.w === w && existing.h === h) return existing;
        if (existing) {
            gl.deleteTexture(existing.tex);
            gl.deleteFramebuffer(existing.fbo);
            targets.delete(name);
        }
        const tex = makeTexture(w, h, linear);
        const fbo = gl.createFramebuffer()!;
        gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
        gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, tex, 0);
        const complete = gl.checkFramebufferStatus(gl.FRAMEBUFFER) === gl.FRAMEBUFFER_COMPLETE;
        gl.bindFramebuffer(gl.FRAMEBUFFER, null);
        if (!complete) {
            console.error("GBA filter framebuffer incomplete");
            gl.deleteTexture(tex);
            gl.deleteFramebuffer(fbo);
            return null;
        }
        const made = { tex, fbo, w, h };
        targets.set(name, made);
        return made;
    }

    /** Uploads the frame; frames[0] then holds it and frames[1..7] the ones before. */
    function uploadFrame(frame: Uint8Array, format: number, w: number, h: number) {
        if (frames.length === 0) {
            for (let i = 0; i <= HISTORY; i++) {
                frames.push(makeTexture(w, h, false));
            }
        }
        // The oldest texture takes the new frame.
        frames = [frames[HISTORY], ...frames.slice(0, HISTORY)];
        const fill = historyFilled ? [frames[0]] : frames;
        for (const tex of fill) {
            gl.bindTexture(gl.TEXTURE_2D, tex);
            gl.texImage2D(gl.TEXTURE_2D, 0, format, w, h, 0, format, gl.UNSIGNED_BYTE, frame);
        }
        historyFilled = true;
    }

    interface PassInput {
        name: string;
        tex: WebGLTexture;
    }

    function pass(
        programName: string,
        inputs: PassInput[],
        out: Target | null,
        outW: number,
        outH: number,
        uniforms: { sourceSize?: [number, number]; videoScale?: number },
    ) {
        const prog = programs.get(programName)!;
        gl.bindFramebuffer(gl.FRAMEBUFFER, out ? out.fbo : null);
        gl.viewport(0, 0, outW, outH);
        gl.useProgram(prog);
        inputs.forEach((input, unit) => {
            gl.activeTexture(gl.TEXTURE0 + unit);
            gl.bindTexture(gl.TEXTURE_2D, input.tex);
            const loc = gl.getUniformLocation(prog, input.name);
            if (loc) gl.uniform1i(loc, unit);
        });
        const set2 = (name: string, v: [number, number] | undefined) => {
            const loc = gl.getUniformLocation(prog, name);
            if (loc && v) gl.uniform2f(loc, v[0], v[1]);
        };
        set2("u_sourceSize", uniforms.sourceSize);
        set2("u_outputSize", [outW, outH]);
        const scaleLoc = gl.getUniformLocation(prog, "u_videoScale");
        if (scaleLoc && uniforms.videoScale !== undefined) gl.uniform1f(scaleLoc, uniforms.videoScale);
        const flipLoc = gl.getUniformLocation(prog, "u_flipY");
        if (flipLoc) gl.uniform1f(flipLoc, out ? 0.0 : 1.0);
        const pos = gl.getAttribLocation(prog, "a_position");
        gl.bindBuffer(gl.ARRAY_BUFFER, deps.positionBuffer());
        gl.enableVertexAttribArray(pos);
        gl.vertexAttribPointer(pos, 2, gl.FLOAT, false, 0, 0);
        gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
    }

    /** Sets the sampling of a texture another pass drew into. */
    function sampleAs(tex: WebGLTexture, linear: boolean) {
        gl.bindTexture(gl.TEXTURE_2D, tex);
        const filter = linear ? gl.LINEAR : gl.NEAREST;
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, filter);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, filter);
    }

    function render(look: GbaLook, frame: Uint8Array, format: number, srcW: number, srcH: number, outW: number, outH: number): boolean {
        for (const [name] of lookPrograms[look]) {
            if (!programs.has(name)) return false;
        }
        uploadFrame(frame, format, srcW, srcH);
        const drawn = draw(look, [srcW, srcH], outW, outH);
        // The other pipelines expect unit 0 active.
        gl.activeTexture(gl.TEXTURE0);
        return drawn;
    }

    function draw(look: GbaLook, src: [number, number], outW: number, outH: number): boolean {
        const [srcW, srcH] = src;
        const current = frames[0];
        switch (look) {
            case "agb001": {
                const big = target("agb001", srcW * 4, srcH * 4, true);
                if (!big) return false;
                pass("agb001", [{ name: "u_texture", tex: current }], big, big.w, big.h, { sourceSize: src });
                sampleAs(big.tex, true);
                pass("stock", [{ name: "u_texture", tex: big.tex }], null, outW, outH, {});
                return true;
            }
            case "nsoGbaColor":
                pass("nso", [{ name: "u_texture", tex: current }], null, outW, outH, {});
                return true;
            case "sp101Color": {
                const lut = images.get("lut");
                const lutOut = target("multiLut", srcW, srcH, false);
                if (!lut || !lutOut) return false;
                pass("multiLut", [{ name: "u_texture", tex: current }, { name: "u_lut", tex: lut }], lutOut, srcW, srcH, {});
                pass("sp101", [{ name: "u_texture", tex: lutOut.tex }], null, outW, outH, {});
                return true;
            }
            case "gbaLcdGrid": {
                const border = images.get("border");
                const response = target("responseTime", srcW, srcH, false);
                const grid = target("lcdGrid", outW, outH, false);
                const colour = target("gbaColor", outW, outH, true);
                if (!border || !response || !grid || !colour) return false;
                const videoScale = lcdGridVideoScale(outW, outH);
                const history = frames.slice(1).map((tex, i) => ({ name: `u_history${i + 1}`, tex }));
                pass("responseTime", [{ name: "u_texture", tex: current }, ...history], response, srcW, srcH, {});
                pass("lcdGrid", [{ name: "u_texture", tex: response.tex }], grid, outW, outH, { sourceSize: src, videoScale });
                pass("gbaColor", [{ name: "u_texture", tex: grid.tex }], colour, outW, outH, {});
                pass("border", [{ name: "u_texture", tex: colour.tex }, { name: "u_border", tex: border }], null, outW, outH, { videoScale });
                return true;
            }
        }
    }

    return { prepare, render };
}
