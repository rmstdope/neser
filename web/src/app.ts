import init, { WasmNes, WasmGb, WasmGba, WasmSnes, gamepad_init_toast_message } from "../pkg/neser";
import { mapStandardGamepadState, selectGamepads } from "./input/gamepad";
import {
    createRomSaveKey,
    hasState,
    loadState,
    openSaveStateDb,
    saveState
} from "./save-state/save_state_storage";
import { createSaveStateController } from "./save-state/save_state_controller";
import { applyJoypadButtonIfAllowed, applyMouseMotion, applyMouseButton, isZapperActive } from "./input/mouse_input";
import { createSaveStateContext } from "./save-state/save_state_context";
import { fetchRomList } from "./rom/rom_list";
import { handleRomSelection } from "./rom/rom_selection";
import { shouldCreateFreshEmulatorForRomStart } from "./rom/emulator_lifecycle";
import { supportedRomExtensionsText, webRomConsoleKindForName, webRomExtensionForName, type WebRomConsoleKind } from "./rom/rom_extensions";
import { createAutorunContext, parseAutorunFile } from "./rom/autorun_context";
import { createFrameLimiter } from "./audio/frame_limiter";
import { computePlaybackRate } from "./audio/audio_resampler";
import { AUDIO_PROFILES, resolveAudioProfileName } from "./audio/audio_profiles";
import { normalizeGbSample, normalizeGbaSample, normalizeNesSample } from "./audio/audio_normalizer";
import { configureEmulatorAudioSampleRate } from "./audio/audio_output_rate";
import { getPlaybackAudioSamples } from "./audio/playback_samples";
import { planFrame } from "./audio/frame_plan";
import { createSineScroller } from "./ui/sine_scroller";
import { getKeyboardControllerTarget } from "./input/input_routing";
import { gbaKeyboardButtonForEvent } from "./input/keyboard_mapping";
import { remapLegacySnesButtonId } from "./input/snes_button_mapping";
import { initTouchControls, isTouchDevice, isHandheldDevice } from "./input/touch_controls";
import { dispatchWebShortcutAction } from "./shortcuts/shortcut_actions";
import {
    buildFullHelpOverlayText,
    buildShortcutReferenceText,
    computeShortcutHelpFontSizePx,
    toggleShortcutHelpVisibility
} from "./shortcuts/shortcut_help";
import { createCrosshair } from "./display/crosshair";
import { computeFullscreenCanvasSize, computeWindowedCanvasSize, computeHandheldCanvasSize } from "./display/canvas_size";
import {
    findNextVisibleZoomHeight,
} from "./display/zoom_controls";
import { createToastContainer, createToastOverlay, drainNesToasts } from "./ui/toast_overlay";
import { createGamepadInitToastNotifier } from "./ui/gamepad_init_toast";
import { renderDisasmLines } from "./debugger/debugger_disasm";
import { buildOamHtml } from "./debugger/debugger_oam";
import { formatWatchEntry, parseWatchAddressInput } from "./debugger/debugger_watch";
import {
    computeNtscDisplayWidth,
    computeScrollViewportRects,
} from "./debugger/ppu_viewer_layout";
import { clampScrollTop, sanitizeScrollTop } from "./debugger/ppu_viewer_scroll";
import { computeMouseCursorStyle } from "./display/cursor_visibility";
import {
    shouldForwardArkanoidMouseInput,
    shouldKeepPointerLocked,
} from "./input/pointer_lock";
import { computeButtonStates } from "./ui/emulation_controls";
import { cycleFilterKey, filterOnConsoleSwitch, type FilterDef } from "./display/filters";
import { selectRenderPipeline } from "./display/render_pipeline";
import commonVertGlsl from "./shaders/common.vert.glsl?raw";
import stockFragGlsl from "./shaders/stock.frag.glsl?raw";
import crtFragGlsl from "./shaders/crt.frag.glsl?raw";
import ntscPass1VertGlsl from "./shaders/ntsc-pass1.vert.glsl?raw";
import ntscPass1FragGlsl from "./shaders/ntsc-pass1.frag.glsl?raw";
import ntscPass2VertGlsl from "./shaders/ntsc-pass2.vert.glsl?raw";
import ntscPass2FragGlsl from "./shaders/ntsc-pass2.frag.glsl?raw";
import gbPass0VertGlsl from "./shaders/gb-pass0.vert.glsl?raw";
import gbPass0FragGlsl from "./shaders/gb-pass0.frag.glsl?raw";
import gbPass1VertGlsl from "./shaders/gb-pass1.vert.glsl?raw";
import gbPass1FragGlsl from "./shaders/gb-pass1.frag.glsl?raw";
import gbBlurVertGlsl from "./shaders/gb-blur.vert.glsl?raw";
import gbPass2FragGlsl from "./shaders/gb-pass2.frag.glsl?raw";
import gbPass3FragGlsl from "./shaders/gb-pass3.frag.glsl?raw";
import gbPass4VertGlsl from "./shaders/gb-pass4.vert.glsl?raw";
import gbPass4FragGlsl from "./shaders/gb-pass4.frag.glsl?raw";

const statusEl = document.getElementById("status");
const fpsCounterEl = document.getElementById("fps-counter");

// Screen Wake Lock and AudioWorklet CPU keepalive were tried but did not
// improve idle-state FPS on Android (S21 FE). The idle/touch FPS gap is caused
// by the kernel input-boost policy, which cannot be replicated from userspace.
const startBtn = document.getElementById("start") as HTMLButtonElement;
const romInput = document.getElementById("rom") as HTMLInputElement;
const romSelect = document.getElementById("rom-select") as HTMLSelectElement | null;
const canvasEl = document.getElementById("screen");
if (!(canvasEl instanceof HTMLCanvasElement)) {
    throw new Error("Canvas element with id 'screen' not found or not a canvas");
}
const canvas: HTMLCanvasElement = canvasEl;
const screenWrap = canvas.closest(".screen-wrap") as HTMLElement;
if (!screenWrap) {
    throw new Error("Screen wrapper with class 'screen-wrap' not found");
}
const shortcutReference = document.getElementById("shortcut-reference");
const shortcutHelpOverlay = document.getElementById("shortcut-help-overlay");
const debuggerPanel = document.getElementById("debugger-panel");

// Use WebGL for rendering with filter support
const gl = canvas.getContext("webgl")!;
if (!gl) {
    throw new Error("WebGL rendering context not available for canvas 'screen'");
}

// NES display dimensions after overscan removal (updated after NES instance is created).
let width = 256 - 2 * 8; // default: horizontal_overscan=8 → 240
let height = 240 - 2 * 8; // default: vertical_overscan=8  → 224
const SCROLLER_TEXT = "May 26, 2026: Version 1.1.0 - GB (DMG+CGB) emulator in ok state. Initial version of AGB emulator.";
const SCROLLER_SPEED = 1.6;
const SCROLLER_AMPLITUDE = 17;
const SCROLLER_FREQUENCY = 0.0587;
const SCROLLER_FONT_SIZE_PX = 15;
const SCROLLER_FONT_FAMILY = "'VT323', monospace";

const toastContainer = createToastContainer(screenWrap);

const toastOverlay = createToastOverlay({ container: toastContainer });

const gamepadInitToastNotifier = createGamepadInitToastNotifier({
    buildMessage: gamepad_init_toast_message,
    showToast: (message) => toastOverlay.show(message)
});

let wasmInitPromise: Promise<unknown> | null = null;

function createWasmUrl() {
    const wasmUrl = new URL("../pkg/neser_bg.wasm", import.meta.url);
    return wasmUrl;
}

function ensureWasmInitialized() {
    if (!wasmInitPromise) {
        wasmInitPromise = init({ module_or_path: createWasmUrl() });
    }
    return wasmInitPromise;
}

// WebGL shader setup for filters
const filters: Record<string, FilterDef> = {
    stock: {
        name: "None",
        type: "single",
        fragmentShader: stockFragGlsl
    },
    ntsc: {
        name: "NTSC",
        type: "ntsc"
    },
    crt: {
        name: "CRT",
        type: "single",
        params: {
            hardScan: -8.0,
            hardPix: -3.0,
            warpX: 0.031,
            warpY: 0.041,
            maskDark: 0.5,
            maskLight: 1.5,
            scaleInLinearGamma: 1.0,
            shadowMask: 3.0,
            brightBoost: 1.0,
            hardBloomScan: -2.0,
            hardBloomPix: -1.5,
            bloomAmount: 0.15,
            shape: 2.0
        },
        fragmentShader: crtFragGlsl
    },
    gameboy: {
        name: "Game Boy",
        type: "gb"
    }
};

// ── GB Dot-Matrix Shader (5-pass) ───────────────────────────────────────
// Ported from vendor/slang-shaders/handheld/gameboy.slangp (GPLv3)
// Original: Copyright (C) 2013 Harlequin, 2024-2025 Matt Akins

// Precision header shared by all GB shaders.
// Fragment shaders probe for highp support; vertex shaders always use highp.
const GB_PREC = `
    #ifdef GL_FRAGMENT_SHADER
        #ifdef GL_FRAGMENT_PRECISION_HIGH
            precision highp float;
        #else
            precision mediump float;
        #endif
    #else
        precision highp float;
    #endif
`;

// Pass 0 vertex — fullscreen mode dot-matrix geometry pre-calculations
const gbPass0VertexSource = GB_PREC + gbPass0VertGlsl;

// Pass 0 fragment — dot-matrix generation + response time + palette
const gbPass0FragmentSource = GB_PREC + gbPass0FragGlsl;

// Pass 1 vertex — pre-compute neighbor texel offsets
const gbPass1VertexSource = GB_PREC + gbPass1VertGlsl;

// Pass 1 fragment — alpha blending between adjacent pixels
const gbPass1FragmentSource = GB_PREC + gbPass1FragGlsl;

// Pass 2 vertex — horizontal blur setup
const gbBlurVertexSource = GB_PREC + gbBlurVertGlsl;

// Pass 2 fragment — horizontal 5-tap Gaussian blur on alpha (sigma=4.0)
const gbPass2FragmentSource = GB_PREC + gbPass2FragGlsl;

// Pass 3 fragment — vertical 5-tap Gaussian blur on alpha (sigma=4.0)
const gbPass3FragmentSource = GB_PREC + gbPass3FragGlsl;

// Pass 4 vertex — resolution scale for shadow compensation
const gbPass4VertexSource = GB_PREC + gbPass4VertGlsl;

// Pass 4 fragment — final compositing: foreground + background + shadows
const gbPass4FragmentSource = GB_PREC + gbPass4FragGlsl;

type ShaderProgram = WebGLProgram & Record<string, unknown>;

// Use the imported FilterDef type from filters.ts (re-exported for local use)

interface AutorunFileInput extends HTMLInputElement {
    _bytes: Uint8Array | null;
    _fileName: string | null;
}

let currentFilter = "ntsc"; // Start with NTSC filter as requested
const filterKeys = Object.keys(filters);
let shaderProgram: ShaderProgram | null = null;
let ntscPass1Program: ShaderProgram | null = null;
let ntscPass2Program: ShaderProgram | null = null;
let ntscPass1Texture: WebGLTexture | null = null;
let ntscPass1Framebuffer: WebGLFramebuffer | null = null;
let ntscPass1TextureType = null;
let ntscPass1Width = width * 4;
let ntscPass1Height = height;
let ntscChromaEncode = 0.0;
const ntscChromaSum = 0.538021759;
let nesTexture: WebGLTexture | null = null;
let positionBuffer: WebGLBuffer | null = null;
let texCoordBuffer: WebGLBuffer | null = null;
// ── GB filter resources ─────────────────────────────────────────────────
let gbPass0Program: ShaderProgram | null = null;
let gbPass1Program: ShaderProgram | null = null;
let gbPass2Program: ShaderProgram | null = null;
let gbPass3Program: ShaderProgram | null = null;
let gbPass4Program: ShaderProgram | null = null;
let gbFbo: (WebGLFramebuffer | null)[] = [null, null, null, null];
let gbTex: (WebGLTexture | null)[] = [null, null, null, null];
let gbFboWidth = 0;
let gbFboHeight = 0;
let gbPrevFrameTex: WebGLTexture | null = null;
let gbPaletteTex: WebGLTexture | null = null;
let gbBackgroundTex: WebGLTexture | null = null;
let gbPrevFrameData: Uint8Array | null = null;
let gbAssetsLoaded = false;
let frameCount = 0; // For NTSC phase animation
const frameLimiter = createFrameLimiter(60);
const idleFrameLimiter = createFrameLimiter(60);
let webglInitialized = false; // Track WebGL initialization state
let idleScrollerActive = false;
let idleScroller: { renderFrame: (ts: number) => Uint8Array } | null = null;
let idleScrollerStartTime = 0;
let crosshair: ReturnType<typeof createCrosshair> | null = null; // Crosshair overlay for Zapper
let windowFocused = true;
let pointerReleasedByEscape = false;
let lockedPointerX = 0;
let lockedPointerY = 0;

function requestPointerLockFromUserGesture() {
    pointerReleasedByEscape = false;
    if (document.pointerLockElement !== canvas) {
        try {
            const pointerLockResult = canvas.requestPointerLock?.();
            pointerLockResult?.catch?.(() => {});
        } catch (_) {
        }
    }
}

function resetWebGLResources() {
    if (nesTexture) gl.deleteTexture(nesTexture);
    if (positionBuffer) gl.deleteBuffer(positionBuffer);
    if (texCoordBuffer) gl.deleteBuffer(texCoordBuffer);
    if (shaderProgram) gl.deleteProgram(shaderProgram);
    if (ntscPass1Program) gl.deleteProgram(ntscPass1Program);
    if (ntscPass2Program) gl.deleteProgram(ntscPass2Program);
    if (ntscPass1Texture) gl.deleteTexture(ntscPass1Texture);
    if (ntscPass1Framebuffer) gl.deleteFramebuffer(ntscPass1Framebuffer);
    nesTexture = null;
    positionBuffer = null;
    texCoordBuffer = null;
    shaderProgram = null;
    ntscPass1Program = null;
    ntscPass2Program = null;
    ntscPass1Texture = null;
    ntscPass1Framebuffer = null;
    ntscPass1TextureType = null;
    // GB resources
    for (const p of [gbPass0Program, gbPass1Program, gbPass2Program, gbPass3Program, gbPass4Program]) {
        if (p) gl.deleteProgram(p);
    }
    for (let i = 0; i < 4; i++) {
        if (gbTex[i]) gl.deleteTexture(gbTex[i]);
        if (gbFbo[i]) gl.deleteFramebuffer(gbFbo[i]);
    }
    if (gbPrevFrameTex) gl.deleteTexture(gbPrevFrameTex);
    // Keep gbPaletteTex and gbBackgroundTex alive (asset textures reusable)
    gbPass0Program = null;
    gbPass1Program = null;
    gbPass2Program = null;
    gbPass3Program = null;
    gbPass4Program = null;
    gbFbo = [null, null, null, null];
    gbTex = [null, null, null, null];
    gbFboWidth = 0;
    gbFboHeight = 0;
    gbPrevFrameTex = null;
}

function cacheProgramLocations(program: ShaderProgram) {
    program._uTextureLocation = gl.getUniformLocation(program, "u_texture");
    program._uTextureSizeLocation = gl.getUniformLocation(program, "u_textureSize");
    program._uOutputSizeLocation = gl.getUniformLocation(program, "u_outputSize");
    program._uFrameCountLocation = gl.getUniformLocation(program, "u_frameCount");
    program._uSourceSizeLocation = gl.getUniformLocation(program, "u_sourceSize");
    program._uChromaEncodeLocation = gl.getUniformLocation(program, "u_chromaEncode");
    program._uChromaSumLocation = gl.getUniformLocation(program, "u_chromaSum");
    program._uHardScanLocation = gl.getUniformLocation(program, "u_hardScan");
    program._uHardPixLocation = gl.getUniformLocation(program, "u_hardPix");
    program._uWarpXLocation = gl.getUniformLocation(program, "u_warpX");
    program._uWarpYLocation = gl.getUniformLocation(program, "u_warpY");
    program._uMaskDarkLocation = gl.getUniformLocation(program, "u_maskDark");
    program._uMaskLightLocation = gl.getUniformLocation(program, "u_maskLight");
    program._uScaleInLinearGammaLocation = gl.getUniformLocation(program, "u_scaleInLinearGamma");
    program._uShadowMaskLocation = gl.getUniformLocation(program, "u_shadowMask");
    program._uBrightBoostLocation = gl.getUniformLocation(program, "u_brightBoost");
    program._uHardBloomScanLocation = gl.getUniformLocation(program, "u_hardBloomScan");
    program._uHardBloomPixLocation = gl.getUniformLocation(program, "u_hardBloomPix");
    program._uBloomAmountLocation = gl.getUniformLocation(program, "u_bloomAmount");
    program._uShapeLocation = gl.getUniformLocation(program, "u_shape");
    program._aPositionLocation = gl.getAttribLocation(program, "a_position");
    program._aTexCoordLocation = gl.getAttribLocation(program, "a_texCoord");
    // GB-specific uniforms
    program._uPrevFrameLocation = gl.getUniformLocation(program, "u_prevFrame");
    program._uColorPaletteLocation = gl.getUniformLocation(program, "u_colorPalette");
    program._uBackgroundLocation = gl.getUniformLocation(program, "u_background");
    program._uGbPass1Location = gl.getUniformLocation(program, "u_gbPass1");
    program._uGbPass1SizeLocation = gl.getUniformLocation(program, "u_gbPass1Size");
}

function createProgram(vertexSource: string, fragmentSource: string) {
    const vertexShader = gl.createShader(gl.VERTEX_SHADER)!;
    gl.shaderSource(vertexShader, vertexSource);
    gl.compileShader(vertexShader);
    if (!gl.getShaderParameter(vertexShader, gl.COMPILE_STATUS)) {
        console.error("Vertex shader compilation failed:", gl.getShaderInfoLog(vertexShader));
        gl.deleteShader(vertexShader);
        return null;
    }

    const fragmentShader = gl.createShader(gl.FRAGMENT_SHADER)!;
    gl.shaderSource(fragmentShader, fragmentSource);
    gl.compileShader(fragmentShader);
    if (!gl.getShaderParameter(fragmentShader, gl.COMPILE_STATUS)) {
        console.error("Fragment shader compilation failed:", gl.getShaderInfoLog(fragmentShader));
        gl.deleteShader(fragmentShader);
        gl.deleteShader(vertexShader);
        return null;
    }

    const program = gl.createProgram()!;
    gl.attachShader(program, vertexShader);
    gl.attachShader(program, fragmentShader);
    gl.linkProgram(program);
    gl.deleteShader(vertexShader);
    gl.deleteShader(fragmentShader);

    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
        console.error("Shader program linking failed:", gl.getProgramInfoLog(program));
        gl.deleteProgram(program);
        return null;
    }

    cacheProgramLocations(program as ShaderProgram);
    return program as ShaderProgram;
}

function createNtscPass1Target() {
    ntscPass1Width = width * 4;
    ntscPass1Height = height;
    const floatTexExt = gl.getExtension("OES_texture_float");
    const colorBufferFloatExt = gl.getExtension("WEBGL_color_buffer_float") || gl.getExtension("EXT_color_buffer_float");
    const useFloat = Boolean(floatTexExt && colorBufferFloatExt);

    ntscPass1TextureType = useFloat ? gl.FLOAT : gl.UNSIGNED_BYTE;
    ntscChromaEncode = useFloat ? 0.0 : 1.0;
    ntscPass1Texture = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, ntscPass1Texture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, ntscPass1Width, ntscPass1Height, 0, gl.RGBA, ntscPass1TextureType, null);

    ntscPass1Framebuffer = gl.createFramebuffer();
    gl.bindFramebuffer(gl.FRAMEBUFFER, ntscPass1Framebuffer);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, ntscPass1Texture, 0);

    const status = gl.checkFramebufferStatus(gl.FRAMEBUFFER);
    if (status !== gl.FRAMEBUFFER_COMPLETE) {
        console.warn("NTSC pass1 framebuffer incomplete, falling back to UNSIGNED_BYTE", status);
        gl.bindTexture(gl.TEXTURE_2D, ntscPass1Texture);
        gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, ntscPass1Width, ntscPass1Height, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
        ntscPass1TextureType = gl.UNSIGNED_BYTE;
        ntscChromaEncode = 1.0;
        const retryStatus = gl.checkFramebufferStatus(gl.FRAMEBUFFER);
        if (retryStatus !== gl.FRAMEBUFFER_COMPLETE) {
            console.error("NTSC pass1 framebuffer still incomplete", retryStatus);
            return false;
        }
    }
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    return true;
}

// ── GB filter setup & asset loading ─────────────────────────────────────

function createGbFbo(w: number, h: number, linear: boolean): { tex: WebGLTexture; fbo: WebGLFramebuffer } | null {
    const tex = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    const filt = linear ? gl.LINEAR : gl.NEAREST;
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, filt);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, filt);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, w, h, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);

    const fbo = gl.createFramebuffer()!;
    gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, tex, 0);
    if (gl.checkFramebufferStatus(gl.FRAMEBUFFER) !== gl.FRAMEBUFFER_COMPLETE) {
        console.error("GB framebuffer incomplete");
        gl.deleteTexture(tex);
        gl.deleteFramebuffer(fbo);
        gl.bindFramebuffer(gl.FRAMEBUFFER, null);
        return null;
    }
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    return { tex, fbo };
}

function ensureGbFbos(w: number, h: number) {
    if (gbFboWidth === w && gbFboHeight === h && gbFbo[0]) return true;
    // Recreate all 4 intermediate FBOs at new size
    for (let i = 0; i < 4; i++) {
        if (gbTex[i]) gl.deleteTexture(gbTex[i]);
        if (gbFbo[i]) gl.deleteFramebuffer(gbFbo[i]);
    }
    for (let i = 0; i < 4; i++) {
        const pair = createGbFbo(w, h, false);
        if (!pair) return false;
        gbTex[i] = pair.tex;
        gbFbo[i] = pair.fbo;
    }
    gbFboWidth = w;
    gbFboHeight = h;
    return true;
}

function loadImageTexture(url: string, linear: boolean): Promise<WebGLTexture | null> {
    return new Promise((resolve) => {
        const img = new Image();
        img.onload = () => {
            const tex = gl.createTexture()!;
            gl.bindTexture(gl.TEXTURE_2D, tex);
            gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, img);
            gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
            gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
            const filt = linear ? gl.LINEAR : gl.NEAREST;
            gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, filt);
            gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, filt);
            resolve(tex);
        };
        img.onerror = () => {
            console.error("Failed to load GB texture:", url);
            resolve(null);
        };
        img.src = url;
    });
}

async function loadGbAssets() {
    if (gbAssetsLoaded && gbPaletteTex && gbBackgroundTex) return true;
    const paletteUrl = new URL("./assets/gb-palette.png", import.meta.url).href;
    const bgUrl = new URL("./assets/gb-background.png", import.meta.url).href;
    const [palette, bg] = await Promise.all([
        loadImageTexture(paletteUrl, false),
        loadImageTexture(bgUrl, true),
    ]);
    if (!palette || !bg) return false;
    gbPaletteTex = palette;
    gbBackgroundTex = bg;
    gbAssetsLoaded = true;
    return true;
}

function setupGbPrograms() {
    const stockProgram = createProgram(commonVertGlsl, stockFragGlsl);
    const pass0Program = createProgram(gbPass0VertexSource, gbPass0FragmentSource);
    const pass1Program = createProgram(gbPass1VertexSource, gbPass1FragmentSource);
    const pass2Program = createProgram(gbBlurVertexSource, gbPass2FragmentSource);
    const pass3Program = createProgram(gbBlurVertexSource, gbPass3FragmentSource);
    const pass4Program = createProgram(gbPass4VertexSource, gbPass4FragmentSource);
    if (!stockProgram || !pass0Program || !pass1Program || !pass2Program || !pass3Program || !pass4Program) {
        for (const program of [stockProgram, pass0Program, pass1Program, pass2Program, pass3Program, pass4Program]) {
            if (program) {
                gl.deleteProgram(program);
            }
        }
        shaderProgram = null;
        gbPass0Program = null;
        gbPass1Program = null;
        gbPass2Program = null;
        gbPass3Program = null;
        gbPass4Program = null;
        return false;
    }
    shaderProgram = stockProgram;
    gbPass0Program = pass0Program;
    gbPass1Program = pass1Program;
    gbPass2Program = pass2Program;
    gbPass3Program = pass3Program;
    gbPass4Program = pass4Program;
    // Create previous-frame texture at source (GB) resolution
    gbPrevFrameTex = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, gbPrevFrameTex);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, width, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);

    // FBOs created lazily in renderGbPass (to match canvas size)
    // Start async asset loading (palette + background PNGs)
    loadGbAssets();
    return true;
}

function renderFrameWithCurrentPipeline(frame: Uint8Array, sourceFormat = gl.RGBA): boolean {
    const pipeline = selectRenderPipeline({
        filterType: filters[currentFilter]?.type,
        gbAssetsLoaded,
        hasSinglePassShader: shaderProgram !== null,
    });

    if (pipeline === "ntsc") {
        return renderNtscPass(frame);
    }

    if (pipeline === "gb") {
        return renderGbPass(frame);
    }

    return renderSinglePass(frame, sourceFormat);
}

function setupFilterPrograms(filterName: string) {
    const filter = (filters as Record<string, FilterDef>)[filterName];
    if (!filter) {
        console.error("Unknown filter:", filterName);
        return false;
    }

    if (filter.type === "ntsc") {
        ntscPass1Program = createProgram(ntscPass1VertGlsl, ntscPass1FragGlsl);
        ntscPass2Program = createProgram(ntscPass2VertGlsl, ntscPass2FragGlsl);
        if (!ntscPass1Program || !ntscPass2Program) {
            return false;
        }
        if (!createNtscPass1Target()) {
            return false;
        }
        shaderProgram = null;
        return true;
    }

    if (filter.type === "gb") {
        return setupGbPrograms();
    }

    shaderProgram = createProgram(commonVertGlsl, filter.fragmentShader!);
    return Boolean(shaderProgram);
}

function initWebGL() {
    // Make this function idempotent - clean up if re-initializing
    if (webglInitialized) {
        resetWebGLResources();
    }

    // Create texture for NES framebuffer
    nesTexture = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, nesTexture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);

    // Allocate texture storage once (we'll update with texSubImage2D per frame)
    allocateFrameTextureStorage();

    // Create vertex buffers for a full-screen quad
    positionBuffer = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, positionBuffer);
    const positions = new Float32Array([
        -1.0, -1.0,
        1.0, -1.0,
        -1.0, 1.0,
        1.0, 1.0
    ]);
    gl.bufferData(gl.ARRAY_BUFFER, positions, gl.STATIC_DRAW);

    texCoordBuffer = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, texCoordBuffer);
    const texCoords = new Float32Array([
        0.0, 1.0,
        1.0, 1.0,
        0.0, 0.0,
        1.0, 0.0
    ]);
    gl.bufferData(gl.ARRAY_BUFFER, texCoords, gl.STATIC_DRAW);

    if (!setupFilterPrograms(currentFilter)) {
        webglInitialized = false;
        return false;
    }

    webglInitialized = true;
    return true;
}

function cycleFilter() {
    const consoleKind = emulator?.kind ?? "nes";
    currentFilter = cycleFilterKey(currentFilter, filterKeys, filters, consoleKind);

    // Recreate buffers and textures but keep running state
    if (!initWebGL()) {
        console.error("Failed to switch filter");
        return false;
    }

    return true;
}

type ActiveEmulator =
    | { kind: "nes"; inst: WasmNes }
    | { kind: "gb"; inst: WasmGb }
    | { kind: "gba"; inst: WasmGba }
    | { kind: "snes"; inst: WasmSnes };

/** Master emulator state — set by the loaded ROM's console kind. */
let emulator: ActiveEmulator | null = null;
/** Convenience alias for NES-specific code paths. Non-null only when emulator.kind === "nes". */
let nes: WasmNes | null = null;

function frameTextureFormat(): number {
    return emulator?.kind === "gba" ? gl.RGB : gl.RGBA;
}

function allocateFrameTextureStorage() {
    const textureFormat = frameTextureFormat();
    gl.texImage2D(gl.TEXTURE_2D, 0, textureFormat, width, height, 0, textureFormat, gl.UNSIGNED_BYTE, null);
}

let romBytes: Uint8Array | null = null;
let romMetadata: { name: string; size: number; bytes: Uint8Array } | null = null;
let saveStateController: { save(): Promise<boolean>; load(): Promise<boolean> } | null = null;
let saveStateAvailable = false;
let running = false;
let paused = false;
let romFromFile = false; // true only when ROM was loaded from the file input

// ── Autorun context + DOM elements ───────────────────────────────────────────
const autorunCtx = createAutorunContext();
const autorunCreateCheckbox = document.getElementById("autorun-create") as HTMLInputElement | null;
const autorunLoadBtn = document.getElementById("autorun-load") as HTMLButtonElement | null;
const autorunStatusEl = document.getElementById("autorun-status");
const autorunFileInput = document.getElementById("autorun-file-input") as AutorunFileInput | null;
const autorunFileInfo = document.getElementById("autorun-file-info");
const autorunFileSummary = document.getElementById("autorun-file-summary");
const autorunCheckpointSelect = document.getElementById("autorun-checkpoint-select") as HTMLSelectElement | null;
const autorunExtendCheck = document.getElementById("autorun-extend-check") as HTMLInputElement | null;
const autorunUseBtn = document.getElementById("autorun-use-btn") as HTMLButtonElement | null;
const autorunCancelBtn = document.getElementById("autorun-cancel");
const autorunModalCancelBtn = document.getElementById("autorun-modal-cancel");
const autorunModalEl = document.getElementById("autorun-modal") as HTMLDialogElement | null;
const recOverlay = document.getElementById("rec-overlay");

/**
 * Update autorun-related UI controls based on current state.
 *
 * - Gates "Create autorun" checkbox: disabled when running or paused.
 * - Refreshes autorun status display.
 */
function updateAutorunControls() {
    // Gate "Create autorun" checkbox: only checkable when stopped and ROM came from file input
    if (autorunCreateCheckbox) {
        autorunCreateCheckbox.disabled = running || paused || !romFromFile;
    }

    updateAutorunStatus();
}

/**
 * Centralized emulation button state update.
 * Computes enabled/disabled and label for Start, Pause, Reset, Stop
 * based on current emulation state, and applies the result to the DOM.
 */
function updateEmulationButtons() {
    const states = computeButtonStates({
        romLoaded: romBytes !== null,
        running,
        paused,
        isRecording: autorunCtx.isCreateRecording(),
    });
    startBtn.disabled = !states.startEnabled;
    startBtn.textContent = states.startLabel;
    // pauseBtn, stopBtn, resetBtn are module-level and non-null (checked at init)
    if (pauseBtn) {
        pauseBtn.disabled = !states.pauseEnabled;
        pauseBtn.textContent = states.pauseLabel;
    }
    if (stopBtn) {
        stopBtn.disabled = !states.stopEnabled;
        stopBtn.textContent = states.stopLabel;
    }
    if (resetBtn) {
        resetBtn.disabled = !states.resetEnabled;
    }
    // Load autorun button: available when a ROM is loaded from file and emulation is stopped
    if (autorunLoadBtn) {
        autorunLoadBtn.disabled = romBytes === null || !romFromFile || running;
    }
}

/** Create a fresh emulator instance and update kind-dependent UI. */
function createEmulatorInstance(kind: WebRomConsoleKind): void {
    resetGamepadState();
    // Free the previous WASM instance to avoid leaking its linear memory.
    emulator?.inst.free();
    nes = null;
    if (kind === "gb") {
        const gb = new WasmGb();
        emulator = { kind: "gb", inst: gb };
    } else if (kind === "gba") {
        const gba = new WasmGba();
        emulator = { kind: "gba", inst: gba };
    } else if (kind === "snes") {
        const snes = new WasmSnes();
        emulator = { kind: "snes", inst: snes };
    } else {
        nes = new WasmNes();
        emulator = { kind: "nes", inst: nes };
    }
    updateNesDisplayDimensions();
    resizeCanvasForCurrentDisplayMode();
    updateEmulatorKindUI();
}

/**
 * Show or hide NES-only UI elements depending on which emulator is active.
 * Called whenever the emulator kind changes (NES ↔ GB).
 */
function updateEmulatorKindUI() {
    const isNes = emulator?.kind === "nes";
    // Debugger panel is NES-only
    if (debuggerPanel) {
        debuggerPanel.style.display = isNes ? "" : "none";
    }
    // Autorun controls are NES-only
    const autorunSection = document.getElementById("autorun-section");
    if (autorunSection) {
        autorunSection.style.display = isNes ? "" : "none";
    }
    // Save-state buttons are NES-only
    const saveStateSection = document.getElementById("save-state-section");
    if (saveStateSection) {
        saveStateSection.style.display = isNes ? "" : "none";
    }
    // Switch to a console-appropriate filter if the current one isn't valid
    const kind = emulator?.kind ?? "nes";
    const newFilter = filterOnConsoleSwitch(currentFilter, filterKeys, filters, kind);
    if (newFilter !== currentFilter) {
        currentFilter = newFilter;
        initWebGL();
    }
    filterToggleBtn.disabled = false;
    updateFilterToggleButtonLabel();
    updateShortcutHelpOverlayText();
}

/** Update the recording overlay (REC : MM:SS) on the canvas. */
function updateRecOverlay() {
    if (!recOverlay) return;
    if (!nes || !nes.autorun_is_recording() || !running) {
        recOverlay.classList.add("hidden");
        recOverlay.setAttribute("aria-hidden", "true");
        return;
    }
    const frames = nes.autorun_recording_frame_count();
    const fps = nes.frame_rate_hz();
    if (!(fps > 0 && Number.isFinite(fps))) {
        recOverlay.classList.add("hidden");
        recOverlay.setAttribute("aria-hidden", "true");
        return;
    }
    const totalSec = Math.floor(frames / fps);
    const mm = String(Math.floor(totalSec / 60)).padStart(2, "0");
    const ss = String(totalSec % 60).padStart(2, "0");
    const timeStr = `REC ${mm}:${ss}`;
    // Only update DOM when the displayed text changes
    if (recOverlay.dataset.recTime !== timeStr) {
        recOverlay.dataset.recTime = timeStr;
        recOverlay.textContent = "";
        const dot = document.createElement("span");
        dot.className = "rec-dot";
        recOverlay.appendChild(dot);
        recOverlay.appendChild(document.createTextNode(timeStr));
    }
    recOverlay.classList.remove("hidden");
    recOverlay.setAttribute("aria-hidden", "false");
}

/** Update the small autorun status text and cancel button in the header. */
function updateAutorunStatus() {
    if (!autorunStatusEl) return;
    const config = autorunCtx.getActiveConfig();
    if (!config) {
        autorunStatusEl.textContent = "";
        autorunCancelBtn?.classList.add("hidden");
    } else if (config.mode === "record") {
        autorunStatusEl.textContent = "";
        autorunCancelBtn?.classList.add("hidden");
    } else {
        const info = autorunCtx.getLoadedFile();
        const cpText = config.checkpointIdx != null
            ? `From checkpoint ${config.checkpointIdx + 1}`
            : "From beginning";
        const expectedRom = autorunCtx.getExpectedRomName();
        const items = [
            `▸ ${info?.frameCount ?? "?"} frames`,
            `▸ ${cpText}`,
            ...(config.extend ? ["▸ Extending"] : []),
            ...(expectedRom ? [`▸ Load ${expectedRom}`] : []),
        ];
        autorunStatusEl.textContent = "";
        for (const t of items) {
            const li = document.createElement("li");
            li.textContent = t;
            autorunStatusEl.appendChild(li);
        }
        autorunCancelBtn?.classList.remove("hidden");
    }
}

if (autorunCreateCheckbox) {
    autorunCreateCheckbox.addEventListener("change", () => {
        autorunCtx.setCreateRecording(autorunCreateCheckbox.checked);
        if (autorunCreateCheckbox.checked) {
            // Clear loaded file when switching to create-recording mode
            autorunCtx.clearLoadedFile();
        }
        updateAutorunControls();
        updateEmulationButtons();
    });
}

if (autorunCancelBtn) {
    autorunCancelBtn.addEventListener("click", () => {
        autorunCtx.clearLoadedFile();
        autorunCtx.setCreateRecording(false);
        if (autorunCreateCheckbox) autorunCreateCheckbox.checked = false;
        updateAutorunControls();
        updateEmulationButtons();
    });
}

// Close dialog on modal Cancel button click
if (autorunModalCancelBtn) {
    autorunModalCancelBtn.addEventListener("click", () => {
        autorunModalEl?.close();
    });
}

// Open modal on "Load autorun" button click
if (autorunLoadBtn) {
    autorunLoadBtn.addEventListener("click", () => {
        // Reset modal state
        if (autorunFileInput) autorunFileInput.value = "";
        if (autorunFileSummary) {
            autorunFileSummary.textContent = "Select an autorun file to inspect checkpoints and playback options.";
        }
        if (autorunFileInfo) autorunFileInfo.classList.remove("hidden");
        if (autorunUseBtn) autorunUseBtn.disabled = true;
        if (autorunExtendCheck) autorunExtendCheck.checked = false;
        if (autorunCheckpointSelect) {
            autorunCheckpointSelect.value = "-1";
            while (autorunCheckpointSelect.options.length > 1) {
                autorunCheckpointSelect.remove(1);
            }
        }
        autorunModalEl?.showModal();
    });
}

// Handle autorun file selection inside modal
if (autorunFileInput) {
    autorunFileInput.addEventListener("change", async (e) => {
        const file = (e.target as HTMLInputElement).files?.[0];
        if (!file || !autorunFileInfo || !autorunFileSummary || !autorunCheckpointSelect || !autorunUseBtn) return;
        try {
            const bytes = new Uint8Array(await file.arrayBuffer());
            const info = parseAutorunFile(bytes);
            const expectedRom = file.name.replace(/\.autorun$/i, ".nes");
            autorunFileSummary.textContent =
                `${file.name} (${info.frameCount} frames, ${info.checkpointCount} checkpoints) · ROM: ${expectedRom}`;
            // Populate checkpoint selector
            while (autorunCheckpointSelect.options.length > 1) {
                autorunCheckpointSelect.remove(1);
            }
            for (let i = 0; i < info.checkpointCount; i++) {
                const opt = document.createElement("option");
                opt.value = String(i);
                opt.textContent = `Checkpoint ${i + 1} (frame ${Math.round((i + 1) * info.frameCount / info.checkpointCount)})`;
                autorunCheckpointSelect.appendChild(opt);
            }
            autorunFileInfo.classList.remove("hidden");
            autorunUseBtn.disabled = false;
            // Store raw bytes and filename on the input element for the Use button
            autorunFileInput._bytes = bytes;
            autorunFileInput._fileName = file.name;
        } catch (err: unknown) {
            autorunFileSummary.textContent = `Error: ${err instanceof Error ? err.message : String(err)}`;
            autorunFileInfo.classList.remove("hidden");
            autorunUseBtn.disabled = true;
            autorunFileInput._bytes = null;
            autorunFileInput._fileName = null;
        }
    });
}

// Handle "Use Autorun" button click
if (autorunUseBtn) {
    autorunUseBtn.addEventListener("click", () => {
        const bytes = autorunFileInput?._bytes;
        if (!bytes) return;
        try {
            autorunCtx.setLoadedFile(bytes, autorunFileInput?._fileName ?? null);
            // Uncheck "Create autorun" since playback takes over
            if (autorunCreateCheckbox) autorunCreateCheckbox.checked = false;
            autorunCtx.setCreateRecording(false);
            const cpVal = autorunCheckpointSelect?.value;
            autorunCtx.setSelectedCheckpoint(cpVal != null && cpVal !== "-1" ? parseInt(cpVal, 10) : null);
            autorunCtx.setExtend(autorunExtendCheck?.checked ?? false);
            updateAutorunStatus();
            // Close modal
            autorunModalEl?.close();
        } catch (err) {
            console.error("Failed to configure autorun:", err);
        }
    });
}

/**
 * Update display dimensions from the active emulator instance and reallocate the GL texture.
 * Must be called after `emulator` is created so the correct resolution is reflected.
 */
function updateNesDisplayDimensions() {
    if (!emulator) return;
    width = emulator.inst.screen_width();
    height = emulator.inst.screen_height();
    NES_ASPECT_RATIO = width / height;
    // Reallocate the texture with the correct dimensions.
    if (nesTexture) {
        gl.bindTexture(gl.TEXTURE_2D, nesTexture);
        allocateFrameTextureStorage();
    }
}
let lastFrameTime = 0;
const fpsLogIntervalMs = 1000;
let fpsLastTime = 0;
let fpsFrames = 0;
let fpsWasmTimeAccMs = 0;
let fpsRenderTimeAccMs = 0;

// Web Audio API setup
let audioContext: AudioContext | null = null;
let nextAudioTime = 0;
const AUDIO_SAMPLE_RATE = 44100; // Target output sample rate for Web Audio (NES audio is downsampled to this rate)
const NES_APU_MAX = 1.177; // Conservative max output from NES APU mixer including expansion audio
const AUDIO_PROFILE_NAME = resolveAudioProfileName(new URLSearchParams(window.location.search).get("audio-profile"));
const AUDIO_PROFILE = AUDIO_PROFILES[AUDIO_PROFILE_NAME];
const AUDIO_TARGET_LATENCY = AUDIO_PROFILE.targetLatencySeconds; // seconds
const AUDIO_MAX_ADJUST = AUDIO_PROFILE.maxAdjust; // +/- playback rate bound
const AUDIO_LATENCY_GAIN = 0.1; // scale factor for latency correction
let audioMuted = false;
let lastGamepadState1 = {
    a: false,
    b: false,
    x: false,
    y: false,
    select: false,
    start: false,
    up: false,
    down: false,
    left: false,
    right: false,
    l: false,
    r: false
};
let lastGamepadState2 = {
    a: false,
    b: false,
    x: false,
    y: false,
    select: false,
    start: false,
    up: false,
    down: false,
    left: false,
    right: false,
    l: false,
    r: false
};

function setStatus(msg: string, isError = false) {
    statusEl!.textContent = isError ? msg : "";
    statusEl!.style.color = isError ? "#f88" : "";
}

async function applyRomBytes(bytes: Uint8Array, name: string) {
    romBytes = bytes;
    romMetadata = {
        name,
        size: romBytes.length,
        bytes: romBytes
    };
    setStatus(`Loaded ROM: ${name} (${romBytes.length} bytes)`);
    stopIdleScroller();
    await refreshSaveStateController();
    updateEmulationButtons();
}

async function refreshSaveStateController() {
    // Save states are NES-only in the web frontend MVP.
    if (!nes || !romMetadata) {
        saveStateController = null;
        saveStateAvailable = false;
        updateSaveStateButtons();
        return;
    }
    try {
        saveStateController = await createSaveStateContext({
            nes,
            romMetadata,
            openDb: openSaveStateDb,
            createRomSaveKey,
            createSaveStateController,
            saveStateFn: saveState,
            loadStateFn: loadState,
            setStatus: (msg, isError = false) => {
                setStatus(msg, isError);
                toastOverlay.show(msg);
            }
        });
        if (saveStateController) {
            const db = await openSaveStateDb();
            const key = await createRomSaveKey({
                name: romMetadata.name,
                size: romMetadata.size,
                bytes: romMetadata.bytes
            });
            saveStateAvailable = await hasState(db, key);
        } else {
            saveStateAvailable = false;
        }
    } catch (error) {
        console.error("Failed to initialize save state", error);
        saveStateController = null;
        saveStateAvailable = false;
        setStatus("Failed to initialize save state", true);
    }
    updateSaveStateButtons();
}

romInput!.addEventListener("change", async (e) => {
    const file = (e.target as HTMLInputElement).files?.[0];
    if (!file) return;
    // Clear bundled ROM selection when a file is chosen
    if (romSelect) romSelect.value = "";
    romFromFile = true;
    requestPointerLockFromUserGesture();
    const expectedRom = autorunCtx.getExpectedRomName();
    if (expectedRom && file.name.toLowerCase() !== expectedRom.toLowerCase()) {
        toastOverlay.show(`⚠ Autorun expects "${expectedRom}" but "${file.name}" was loaded — playback may not work correctly`);
    }
    await handleRomSelection({
        bytes: new Uint8Array(await file.arrayBuffer()),
        name: file.name,
        running,
        stop,
        applyRomBytes,
        start,
        focusCanvas: () => canvas.focus()
    });
});

if (romSelect) {
    romSelect.addEventListener("change", async (e) => {
        const sel = e.target as HTMLSelectElement;
        const value = sel.value;
        if (!value) return;
        // Clear file input when a bundled ROM is selected
        romInput.value = "";
        romFromFile = false;
        requestPointerLockFromUserGesture();
        try {
            const response = await fetch(value);
            if (!response.ok) {
                throw new Error(`HTTP ${response.status}`);
            }
            const bytes = new Uint8Array(await response.arrayBuffer());
            // Prefer the option's display text (e.g. "cpu.nes") over URL parsing,
            // since data-URL values don't contain a meaningful filename.
            const selectedOption = sel.options[sel.selectedIndex];
            const name = selectedOption?.textContent?.trim() || value.split("/").pop() || value;
            await handleRomSelection({
                bytes,
                name,
                running,
                stop,
                applyRomBytes,
                start,
                focusCanvas: () => canvas.focus()
            });
        } catch (error) {
            console.error("Failed to load bundled ROM", error);
            setStatus("Failed to load bundled ROM", true);
        }
    });
}

function clearCanvas() {
    gl.clearColor(0.0, 0.0, 0.0, 1.0);
    gl.clear(gl.COLOR_BUFFER_BIT);
}

async function initAudioContext(): Promise<void> {
    if (!audioContext) {
        // Create AudioContext on first user interaction (required by browsers)
        audioContext = new (window.AudioContext || (window as any).webkitAudioContext)({
            sampleRate: AUDIO_SAMPLE_RATE
        });
        nextAudioTime = audioContext.currentTime;
        console.log(`Audio initialized: ${audioContext.sampleRate} Hz`);
    }
}

function configureActiveEmulatorAudioSampleRate() {
    if (!emulator || !audioContext) {
        return;
    }
    const configured = configureEmulatorAudioSampleRate(emulator.inst, audioContext.sampleRate);
    if (!configured) {
        console.warn("Unable to configure emulator audio sample rate");
    }
}

function playAudioSamples(samples: Float32Array, channels = 1) {
    if (!audioContext || audioMuted || samples.length === 0) return;

    const channelCount = channels === 2 ? 2 : 1;
    const frameCount = channelCount === 2 ? Math.floor(samples.length / 2) : samples.length;
    if (frameCount === 0) return;

    // Create an audio buffer for the samples
    const buffer = audioContext.createBuffer(channelCount, frameCount, audioContext.sampleRate);
    const channelData = buffer.getChannelData(0);

    // Normalize and copy samples to the buffer
    if (channelCount === 2) {
        const rightChannelData = buffer.getChannelData(1);
        for (let i = 0; i < frameCount; i++) {
            channelData[i] = normalizeGbaSample(samples[i * 2]);
            rightChannelData[i] = normalizeGbaSample(samples[i * 2 + 1]);
        }
    } else if (emulator?.kind === "gb" || emulator?.kind === "gba" || emulator?.kind === "snes") {
        // GB, GBA, and SNES APUs all output bipolar samples in [-1.0, 1.0].
        // GBA and SNES share normalizeGbaSample (clamp to [-1, 1]); GB uses its own normalizer.
        for (let i = 0; i < frameCount; i++) {
            channelData[i] = emulator?.kind === "gb"
                ? normalizeGbSample(samples[i])
                : normalizeGbaSample(samples[i]);
        }
    } else {
        // NES APU outputs 0.0 to ~1.177; normalize to the unipolar 0.0 to 1.0 range used by this output path
        for (let i = 0; i < frameCount; i++) {
            channelData[i] = normalizeNesSample(samples[i], NES_APU_MAX);
        }
    }

    // Create a buffer source and schedule it
    const source = audioContext.createBufferSource();
    source.buffer = buffer;
    source.connect(audioContext.destination);

    // Schedule playback at the next available time
    // Ensure smooth continuous playback by scheduling samples back-to-back
    const now = audioContext.currentTime;
    if (nextAudioTime < now) {
        nextAudioTime = now;
    }
    // Cap how far ahead of real-time we schedule audio to avoid unbounded latency
    const MAX_AUDIO_LOOKAHEAD = 0.5; // seconds
    if (nextAudioTime - now > MAX_AUDIO_LOOKAHEAD) {
        nextAudioTime = now + MAX_AUDIO_LOOKAHEAD;
    }
    const latencySeconds = Math.max(0, nextAudioTime - now);
    const playbackRate = computePlaybackRate({
        latencySeconds,
        targetLatencySeconds: AUDIO_TARGET_LATENCY,
        maxAdjust: AUDIO_MAX_ADJUST,
        gain: AUDIO_LATENCY_GAIN
    });
    source.playbackRate.value = playbackRate;
    source.start(nextAudioTime);
    nextAudioTime += buffer.duration / playbackRate;
}

async function start() {
    // Prevent concurrent starts by disabling the button immediately.
    if (startBtn!.disabled) {
        return;
    }
    startBtn!.disabled = true;
    if (!romBytes) {
        setStatus("Please choose a ROM first", true);
        updateEmulationButtons();
        return;
    }
    const romName = romMetadata?.name ?? "selected-rom.nes";
    const consoleKind = webRomConsoleKindForName(romName);

    // Reject unsupported file types before any async work.
    if (!consoleKind) {
        const ext = webRomExtensionForName(romName);
        toastOverlay.show(`Unsupported file type .${ext} — only ${supportedRomExtensionsText()} are supported`);
        setStatus(`Unsupported file type .${ext}`, true);
        updateEmulationButtons();
        return;
    }

    stopIdleScroller();
    setStatus("Initializing emulator...");
    try {
        if (!emulator) {
            await ensureWasmInitialized();

            // Initialize WebGL shaders before creating the emulator instance
            if (!initWebGL()) {
                throw new Error("Failed to initialize WebGL");
            }
        }

        if (shouldCreateFreshEmulatorForRomStart(emulator?.kind ?? null, consoleKind)) {
            createEmulatorInstance(consoleKind);
        }

        // ── NES-only: Autorun setup (configure before loading ROM) ──────────
        if (nes) {
            const autorunConfig = autorunCtx.getActiveConfig();
            if (autorunConfig?.mode === "record") {
                nes.start_autorun_recording();
            } else {
                nes.clear_autorun();
            }
        }

        emulator!.inst.load_rom(romBytes, romName);
        drainNesToasts(emulator?.inst ?? null, toastOverlay);

        // ── NES-only: Autorun setup (playback/extend – after ROM is loaded) ──
        if (nes) {
            const autorunConfig = autorunCtx.getActiveConfig();
            if (autorunConfig?.mode === "playback") {
                try {
                    const pendingRestore = nes.load_autorun_playback(
                        autorunConfig.bytes!,
                        autorunConfig.checkpointIdx ?? -1,
                        autorunConfig.extend ?? false
                    );
                    if (pendingRestore && pendingRestore.length > 0) {
                        nes.load_state_bytes(pendingRestore);
                    }
                } catch (autorunErr) {
                    console.error("Failed to load autorun for playback:", autorunErr);
                    toastOverlay.show(`Autorun load failed: ${autorunErr}`);
                    nes.clear_autorun();
                }
            }
        }

        frameLimiter.setTargetFps(emulator!.inst.frame_rate_hz());
        // Initialize audio context on user interaction (browser requirement)
        await initAudioContext();
        configureActiveEmulatorAudioSampleRate();
        emulator!.inst.set_audio_muted(audioMuted);
        await refreshSaveStateController();

        // Update pointer visibility and Zapper overlay after ROM is loaded
        updateMouseCursorState();
    } catch (err: unknown) {
        drainNesToasts(emulator?.inst ?? null, toastOverlay);
        setStatus(`Failed to load ROM: ${err}`, true);
        updateEmulationButtons();
        // Only reset emulator if wasm/webgl initialization failed
        // Don't reset on simple ROM load errors so we can retry
        if (err instanceof Error && err.message.includes("WebGL")) {
            emulator = null;
            nes = null;
            webglInitialized = false;
        }
        return;
    }
    running = true;
    paused = false;
    if (isTouchDevice()) document.body.classList.add("touch-running");
    if (isHandheldDevice()) updateHandheldCanvasSize();
    const sidebarToggle = document.getElementById("sidebar-toggle") as HTMLInputElement | null;
    if (sidebarToggle) sidebarToggle.checked = false;
    setStatus("Running...");
    updateAutorunControls();
    updateEmulationButtons();
    updateSaveStateButtons();
    requestAnimationFrame(step);
}

function resumeFrameLoop() {
    lastFrameTime = 0;
    frameLimiter.reset();
    setStatus("Running...");
    requestAnimationFrame(step);
}

function pauseResume() {
    if (!emulator || !running) return;
    paused = !paused;
    if (!paused) {
        resumeFrameLoop();
    } else {
        setStatus("Paused");
    }
    updateAutorunControls();
    updateEmulationButtons();
}

let debuggerHexdumpError = "";
let debuggerWatchAddError = "";
let debuggerWatchRowErrors = new Map();

function buildDisasmHtml(nes: WasmNes) {
    try {
        const disasmJson = nes.debugger_disasm_json();
        const lines = JSON.parse(disasmJson);
        return renderDisasmLines(lines);
    } catch (_) { /* disasm not available yet */ }
    return "";
}

function formatStatusFlags(p: number) {
    const flag = (bit: number, ch: string) => (p & (1 << bit)) ? ch : "-";
    return flag(7,"N") + flag(6,"V") + flag(5,"U") + flag(4,"B") +
           flag(3,"D") + flag(2,"I") + flag(1,"Z") + flag(0,"C");
}

function buildRegsHtml(snap: Record<string, any>) {
    const h2 = (n: number) => n.toString(16).toUpperCase().padStart(2, "0");
    const h4 = (n: number) => n.toString(16).toUpperCase().padStart(4, "0");
    const intStr = snap.interrupt === null ? "-" :
                   snap.interrupt === "nmi" ? "NMI" : "IRQ";
    const lines = [
        `PC: ${h4(snap.pc)}  SP: ${h2(snap.sp)}`,
        `A:  ${h2(snap.a)}  X:  ${h2(snap.x)}  Y:  ${h2(snap.y)}`,
        `P:  ${h2(snap.p)}  ${formatStatusFlags(snap.p)}`,
        `INT: ${intStr}`,
        `VEC NMI:${h4(snap.nmi_vector)} RST:${h4(snap.reset_vector)} IRQ:${h4(snap.irq_vector)}`,
        `CYC: ${snap.cycles}`,
        `Frame:${snap.frame_count}  Scanline:${snap.scanline}  Pixel:${snap.pixel}`,
    ];
    return lines.map((l: string) => {
        const esc = l.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
        return `<span>${esc}</span>`;
    }).join("\n");
}

function formatHexdumpLines(baseAddr: number, bytes: number[]) {
    const lines = [];
    const safeBytes = Array.isArray(bytes) ? bytes : [];
    for (let row = 0; row < Math.ceil(safeBytes.length / 16); row++) {
        const addr = (baseAddr + row * 16) & 0xFFFF;
        const chunk = safeBytes.slice(row * 16, row * 16 + 16);
        const hexParts = [];
        for (let column = 0; column < 16; column++) {
            const value = chunk[column];
            hexParts.push(value === undefined ? "  " : value.toString(16).toUpperCase().padStart(2, "0"));
        }
        const ascii = chunk
            .map((value: number) => (value >= 0x20 && value <= 0x7E ? String.fromCharCode(value) : "."))
            .join("");
        lines.push(`${addr.toString(16).toUpperCase().padStart(4, "0")}: ${hexParts.join(" ")} |${ascii}|`);
    }
    return lines;
}

function buildHexdumpHtml(snap: Record<string, any>) {
    const base = Number.isInteger(snap.prg_hexdump_base) ? snap.prg_hexdump_base : 0;
    const lines = formatHexdumpLines(base, snap.prg_hexdump_bytes);
    const escLine = (line: string) => line.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
    const linesHtml = lines.map((line) => `<span>${escLine(line)}</span>`).join("\n");
    const baseHex = base.toString(16).toUpperCase().padStart(4, "0");
    const errorHtml = debuggerHexdumpError
        ? `<span class="debugger-hexdump-error">${escLine(debuggerHexdumpError)}</span>`
        : "";
    return (
        `<div class="debugger-hexdump-controls">` +
        `<button class="dbg-btn" id="dbg-hexdump-prev">-16</button>` +
        `<button class="dbg-btn" id="dbg-hexdump-next">+16</button>` +
        `<input class="dbg-hexdump-input" id="dbg-hexdump-base" value="${baseHex}" />` +
        `<button class="dbg-btn" id="dbg-hexdump-go">Go</button>` +
        `</div>` +
        `${errorHtml}` +
        `<span class="debugger-hexdump-title">PRG-ROM hexdump @ ${baseHex}</span>` +
        `<span class="debugger-hexdump-block">${linesHtml}</span>`
    );
}

function buildWatchHtml(snap: Record<string, any>) {
    const watchValues = Array.isArray(snap.watch_values) ? snap.watch_values : [];
    const esc = (value: unknown) => String(value)
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;");

    const rows = watchValues.map((entry: Record<string, any>, index: number) => {
        const address = Number(entry?.address) & 0xFFFF;
        const value = Number(entry?.value) & 0xFF;
        const text = esc(formatWatchEntry(address, value));
        const addrHex = address.toString(16).toUpperCase().padStart(4, "0");
        const rowError = debuggerWatchRowErrors.get(index);
        const rowErrorHtml = rowError
            ? `<span class="debugger-watch-error">${esc(rowError)}</span>`
            : "";
        return (
            `<div class="debugger-watch-row">` +
            `<input class="dbg-watch-input" id="dbg-watch-addr-${index}" value="${addrHex}" />` +
            `<span class="debugger-watch-value">${text}</span>` +
            `<button class="dbg-btn" id="dbg-watch-rm-${index}">X</button>` +
            `</div>` +
            rowErrorHtml
        );
    }).join("");

    const addErrorHtml = debuggerWatchAddError
        ? `<span class="debugger-watch-error">${esc(debuggerWatchAddError)}</span>`
        : "";

    return (
        `<span class="debugger-watch-title">Memory Watch</span>` +
        `<div class="debugger-watch-controls">` +
        `<input class="dbg-watch-input" id="dbg-watch-add-input" placeholder="addr (hex)" />` +
        `<button class="dbg-btn" id="dbg-watch-add">Add</button>` +
        `</div>` +
        addErrorHtml +
        `<div class="debugger-watch-block">${rows}</div>`
    );
}

function buildTraceHtml(snap: Record<string, any>) {
    const traceLines = Array.isArray(snap.recent_trace) ? snap.recent_trace : [];
    const esc = (value: unknown) => String(value)
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;");

    const rows = traceLines.map((entry: Record<string, any>) => {
        const addr = (Number(entry?.addr) & 0xFFFF).toString(16).toUpperCase().padStart(4, "0");
        const bytes = Array.isArray(entry?.bytes)
            ? entry.bytes.map((value) => (Number(value) & 0xFF).toString(16).toUpperCase().padStart(2, "0")).join(" ")
            : "";
        const text = typeof entry?.text === "string" ? entry.text : "";
        return `<span class="debugger-trace-row">${esc(`${addr}: ${bytes.padEnd(8, " ")} ${text}`)}</span>`;
    }).join("");

    return (
        `<span class="debugger-trace-title">Trace (recent 32)</span>` +
        `<div class="debugger-trace-block">${rows}</div>`
    );
}

const PPU_PATTERN_CANVAS_ID = "dbg-ppu-pattern";
const PPU_NAMETABLES_CANVAS_ID = "dbg-ppu-nametables";
const PPU_SECTION_ID = "dbg-ppu-section";
const PPU_PATTERN_WIDTH = 256;
const PPU_PATTERN_HEIGHT = 128;
const PPU_NAMETABLES_WIDTH = 512;
const PPU_NAMETABLES_HEIGHT = 480;
const PPU_NAMETABLES_DISPLAY_WIDTH = computeNtscDisplayWidth(PPU_NAMETABLES_WIDTH);
const PPU_VIEWPORT_STROKE_STYLE = "rgba(255, 255, 0, 1)";
const PPU_VIEWPORT_LINE_WIDTH = 2;
let debuggerPpuViewerScrollTop = 0;

function drawRgbaToCanvas(canvasId: string, rgbaBytes: Uint8Array | null, width: number, height: number, displayWidth = width) {
    const canvasEl = document.getElementById(canvasId);
    if (!(canvasEl instanceof HTMLCanvasElement)) {
        return;
    }
    const context = canvasEl.getContext("2d");
    if (!context) {
        return;
    }

    canvasEl.width = displayWidth;
    canvasEl.height = height;
    const expectedLength = width * height * 4;
    if (!rgbaBytes || rgbaBytes.length !== expectedLength) {
        context.clearRect(0, 0, displayWidth, height);
        return;
    }

    if (displayWidth === width) {
        const imageData = context.createImageData(width, height);
        imageData.data.set(rgbaBytes);
        context.putImageData(imageData, 0, 0);
        return;
    }

    const sourceCanvas = document.createElement("canvas");
    sourceCanvas.width = width;
    sourceCanvas.height = height;
    const sourceContext = sourceCanvas.getContext("2d");
    if (!sourceContext) {
        return;
    }

    const imageData = sourceContext.createImageData(width, height);
    imageData.data.set(rgbaBytes);
    sourceContext.putImageData(imageData, 0, 0);

    context.imageSmoothingEnabled = false;
    context.clearRect(0, 0, displayWidth, height);
    context.drawImage(sourceCanvas, 0, 0, displayWidth, height);
}

function renderPpuViewerCanvases() {
    if (!nes || !nes.debugger_is_ppu_viewer_open()) {
        return;
    }

    try {
        drawRgbaToCanvas(
            PPU_PATTERN_CANVAS_ID,
            nes.debugger_ppu_pattern_tables_rgba(),
            PPU_PATTERN_WIDTH,
            PPU_PATTERN_HEIGHT
        );
        drawRgbaToCanvas(
            PPU_NAMETABLES_CANVAS_ID,
            nes.debugger_ppu_nametables_rgba(),
            PPU_NAMETABLES_WIDTH,
            PPU_NAMETABLES_HEIGHT,
            PPU_NAMETABLES_DISPLAY_WIDTH
        );
        drawPpuViewportRectangles();
    } catch (_) {
        // PPU viewer data is best-effort while emulator/debugger initializes.
    }
}

function drawPpuViewportRectangles() {
    const canvasEl = document.getElementById(PPU_NAMETABLES_CANVAS_ID);
    if (!(canvasEl instanceof HTMLCanvasElement)) {
        return;
    }
    const context = canvasEl.getContext("2d");
    if (!context) {
        return;
    }

    const scrollJson = nes!.debugger_ppu_scroll_json();
    const scroll = JSON.parse(scrollJson);
    const scrollX = Number.isInteger(scroll.scroll_x) ? scroll.scroll_x : 0;
    const scrollY = Number.isInteger(scroll.scroll_y) ? scroll.scroll_y : 0;

    const scaleX = canvasEl.width / PPU_NAMETABLES_WIDTH;
    const scaleY = canvasEl.height / PPU_NAMETABLES_HEIGHT;
    const rects = computeScrollViewportRects(scrollX, scrollY);

    context.save();
    context.strokeStyle = PPU_VIEWPORT_STROKE_STYLE;
    context.lineWidth = PPU_VIEWPORT_LINE_WIDTH;
    for (const rect of rects) {
        context.strokeRect(
            rect.x * scaleX,
            rect.y * scaleY,
            rect.width * scaleX,
            rect.height * scaleY
        );
    }
    context.restore();
}

function buildPpuViewerHtml(isVisible: boolean) {
    if (!isVisible) {
        return "";
    }
    return (
        `<div class="debugger-ppu-overlay">` +
        `<span class="debugger-ppu-title">PPU Viewer</span>` +
        `<div class="debugger-ppu-section" id="${PPU_SECTION_ID}">` +
        `<span class="debugger-ppu-label">Pattern tables</span>` +
        `<canvas id="${PPU_PATTERN_CANVAS_ID}" class="debugger-ppu-canvas"></canvas>` +
        `<span class="debugger-ppu-label">Nametables</span>` +
        `<canvas id="${PPU_NAMETABLES_CANVAS_ID}" class="debugger-ppu-canvas debugger-ppu-canvas-large"></canvas>` +
        `</div>` +
        `</div>`
    );
}

function syncPpuViewerScrollState() {
    const section = document.getElementById(PPU_SECTION_ID);
    if (!(section instanceof HTMLElement)) {
        return;
    }

    debuggerPpuViewerScrollTop = sanitizeScrollTop(debuggerPpuViewerScrollTop);
    section.scrollTop = clampScrollTop(debuggerPpuViewerScrollTop, {
        scrollHeight: section.scrollHeight,
        clientHeight: section.clientHeight,
    });

    section.addEventListener("scroll", () => {
        debuggerPpuViewerScrollTop = sanitizeScrollTop(section.scrollTop);
    });
}

function updateDebuggerPanel() {
    if (!nes || !debuggerPanel) return;
    let snap;
    try {
        snap = JSON.parse(nes.debugger_snapshot_json());
    } catch (_) {
        return;
    }

    const disasmHtml = buildDisasmHtml(nes);
    const traceHtml = buildTraceHtml(snap);
    const regsHtml = buildRegsHtml(snap);
    const hexdumpHtml = buildHexdumpHtml(snap);
    const oamHtml = buildOamHtml(snap.oam);
    const watchHtml = buildWatchHtml(snap);
    const ppuViewerVisible = nes.debugger_is_ppu_viewer_open();
    const ppuViewerHtml = buildPpuViewerHtml(ppuViewerVisible);
    const ppuViewerButtonText = ppuViewerVisible ? "Hide PPU Viewer" : "Show PPU Viewer";

    debuggerPanel.innerHTML =
        `<div class="debugger-controls">` +
        `<div class="debugger-controls-upper">` +
        `<button class="dbg-btn" id="dbg-continue">Continue (F5)</button>` +
        `<button class="dbg-btn" id="dbg-step-over">Step over (F10)</button>` +
        `<button class="dbg-btn" id="dbg-step-into">Step into (F11)</button>` +
        `<span class="dbg-spacer"></span>` +
        `<button class="dbg-btn" id="dbg-toggle-ppu-viewer">${ppuViewerButtonText}</button>` +
        `</div>` +
        `<div class="debugger-controls-lower">` +
        `<button class="dbg-btn" id="dbg-run-next-frame">Run to next frame</button>` +
        `<button class="dbg-btn" id="dbg-run-next-scanline">Run to next scanline</button>` +
        `<button class="dbg-btn" id="dbg-run-to-nmi">Run to NMI</button>` +
        `<button class="dbg-btn" id="dbg-run-to-irq">Run to IRQ</button>` +
        `</div>` +
        `</div>` +
        `<div class="debugger-body">` +
        `<div class="debugger-disasm">` +
        `<span class="debugger-disasm-title">Code</span>` +
        `<span class="disasm-block">${disasmHtml}</span>` +
        `<span class="debugger-hexdump-divider"></span>` +
        `${traceHtml}` +
        `</div>` +
        `<div class="debugger-regs">` +
        `<div class="debugger-regs-scroll">` +
        `<span class="debugger-regs-title">Registers</span>` +
        `<span class="debugger-regs-block">${regsHtml}</span>` +
        `<span class="debugger-hexdump-divider"></span>` +
        `${hexdumpHtml}` +
        `<span class="debugger-hexdump-divider"></span>` +
        `${oamHtml}` +
        `<span class="debugger-hexdump-divider"></span>` +
        `${watchHtml}` +
        `</div>` +
        `</div>` +
        `${ppuViewerHtml}` +
        `</div>`;

    // Wire up buttons (re-attached after each innerHTML update)
    wireDebuggerButtons();
    renderPpuViewerCanvases();
    syncPpuViewerScrollState();
}

function wireDebuggerButtons() {
    function wire(id: string, handler: () => void) {
        document.getElementById(id)?.addEventListener("click", (e) => { e.stopPropagation(); handler(); });
    }
    wire("dbg-step-over", debuggerStepOver);
    wire("dbg-step-into", debuggerStepInto);
    wire("dbg-continue", debuggerClose);
    wire("dbg-run-next-frame", debuggerRunToNextFrame);
    wire("dbg-run-next-scanline", debuggerRunToNextScanline);
    wire("dbg-run-to-nmi", debuggerRunToNmi);
    wire("dbg-run-to-irq", debuggerRunToIrq);
    wire("dbg-toggle-ppu-viewer", debuggerTogglePpuViewer);
    wire("dbg-hexdump-prev", debuggerHexdumpPrev16);
    wire("dbg-hexdump-next", debuggerHexdumpNext16);
    wire("dbg-hexdump-go", debuggerHexdumpGoToAddress);
    document.getElementById("dbg-hexdump-base")?.addEventListener("keydown", (e) => {
        if (e.key === "Enter") {
            e.preventDefault();
            debuggerHexdumpGoToAddress();
        }
    });

    wire("dbg-watch-add", debuggerWatchAddAddress);
    document.getElementById("dbg-watch-add-input")?.addEventListener("keydown", (e) => {
        if (e.key === "Enter") {
            e.preventDefault();
            debuggerWatchAddAddress();
        }
    });

    document.querySelectorAll("[id^='dbg-watch-rm-']").forEach((el) => {
        el.addEventListener("click", (e) => {
            e.stopPropagation();
            const index = Number(el.id.replace("dbg-watch-rm-", ""));
            if (Number.isInteger(index)) {
                debuggerWatchRemoveAddress(index);
            }
        });
    });

    document.querySelectorAll("[id^='dbg-watch-addr-']").forEach((el) => {
        el.addEventListener("keydown", (e: Event) => {
            if ((e as KeyboardEvent).key !== "Enter") {
                return;
            }
            e.preventDefault();
            const index = Number(el.id.replace("dbg-watch-addr-", ""));
            if (!Number.isInteger(index)) {
                return;
            }
            debuggerWatchUpdateAddress(index, (el as HTMLInputElement).value);
        });
    });
}

function debuggerWatchAddAddress() {
    if (!nes) return;
    const input = document.getElementById("dbg-watch-add-input");
    if (!(input instanceof HTMLInputElement)) return;

    const parsed = parseWatchAddressInput(input.value);
    if (parsed === null) {
        debuggerWatchAddError = "Invalid watch address";
        updateDebuggerPanel();
        return;
    }

    debuggerWatchAddError = "";
    nes.debugger_watch_add(parsed);
    updateDebuggerPanel();
}

function debuggerWatchRemoveAddress(index: number) {
    if (!nes) return;
    debuggerWatchRowErrors.delete(index);
    nes.debugger_watch_remove(index);
    updateDebuggerPanel();
}

function debuggerWatchUpdateAddress(index: number, value: string) {
    if (!nes) return;
    const parsed = parseWatchAddressInput(value);
    if (parsed === null) {
        debuggerWatchRowErrors.set(index, "Invalid watch address");
        updateDebuggerPanel();
        return;
    }

    debuggerWatchRowErrors.delete(index);
    nes.debugger_watch_update(index, parsed);
    updateDebuggerPanel();
}

function showDebuggerPanel() {
    if (!debuggerPanel) return;
    debuggerPanel.classList.remove("hidden");
    updateDebuggerPanel();
    setStatus("Debugger paused");
}

function hideDebuggerPanel() {
    if (debuggerPanel) {
        debuggerPanel.classList.add("hidden");
    }
}

function debuggerOpen() {
    if (!nes) return;
    nes.debugger_open();
    showDebuggerPanel();
}

function debuggerClose() {
    if (!nes) return;
    nes.debugger_continue();
    hideDebuggerPanel();
    if (!paused) {
        resumeFrameLoop();
    }
}

function debuggerToggle() {
    if (!nes || !running) return;
    if (nes.is_debugger_open()) {
        debuggerClose();
    } else {
        debuggerOpen();
    }
}

function debuggerStepOver() {
    if (!nes || !running) return;
    nes.debugger_step_over();
    showDebuggerPanel();
}

function debuggerStepInto() {
    if (!nes || !running) return;
    nes.debugger_step_into();
    showDebuggerPanel();
}

function cyclePaletteAction() {
    if (!nes) return;
    nes.cycle_palette();
    drainNesToasts(nes, toastOverlay);
}

function debuggerRunToNextFrame() {
    if (!nes || !running) return;
    nes.debugger_run_to_next_frame();
    showDebuggerPanel();
}

function debuggerRunToNextScanline() {
    if (!nes || !running) return;
    nes.debugger_run_to_next_scanline();
    showDebuggerPanel();
}

function debuggerRunToNmi() {
    if (!nes || !running) return;
    nes.debugger_run_to_nmi();
    showDebuggerPanel();
}

function debuggerRunToIrq() {
    if (!nes || !running) return;
    nes.debugger_run_to_irq();
    showDebuggerPanel();
}

function debuggerTogglePpuViewer() {
    if (!nes || !running) return;
    nes.debugger_toggle_ppu_viewer();
    showDebuggerPanel();
}

function debuggerHexdumpPrev16() {
    if (!nes || !running) return;
    debuggerHexdumpError = "";
    nes.debugger_hexdump_prev_16();
    showDebuggerPanel();
}

function debuggerHexdumpNext16() {
    if (!nes || !running) return;
    debuggerHexdumpError = "";
    nes.debugger_hexdump_next_16();
    showDebuggerPanel();
}

function parseHexdumpAddressInput(rawInput: string) {
    const normalized = rawInput.trim().replace(/^0x/i, "");
    if (!/^[0-9a-fA-F]+$/.test(normalized)) {
        return { ok: false, error: "Invalid address" };
    }
    const parsed = Number.parseInt(normalized, 16);
    if (!Number.isInteger(parsed) || parsed < 0x8000 || parsed > 0xFFFF) {
        return { ok: false, error: "Address must be in 8000-FFFF" };
    }
    return { ok: true, value: parsed };
}

function debuggerHexdumpGoToAddress() {
    if (!nes || !running) return;
    const input = document.getElementById("dbg-hexdump-base");
    if (!(input instanceof HTMLInputElement)) return;

    const parsed = parseHexdumpAddressInput(input.value);
    if (!parsed.ok) {
        debuggerHexdumpError = parsed.error || "";
        showDebuggerPanel();
        return;
    }

    debuggerHexdumpError = "";
    nes!.debugger_hexdump_set_base(parsed.value!);
    showDebuggerPanel();
}

function stop() {
    // ── NES-only: Autorun teardown ────────────────────────────────────────
    if (nes && nes.autorun_is_recording()) {
        const recordingBytes = nes.stop_autorun();
        if (recordingBytes && recordingBytes.length > 0) {
            triggerAutorunDownload(recordingBytes, romMetadata?.name);
        }
    } else if (nes) {
        nes.clear_autorun();
    }

    // Auto-uncheck "Create autorun" after recording stops
    autorunCtx.setCreateRecording(false);
    if (autorunCreateCheckbox) autorunCreateCheckbox.checked = false;

    running = false;
    paused = false;
    document.body.classList.remove("touch-running");
    clearCanvas();
    lastFrameTime = 0;
    if (fpsCounterEl) (fpsCounterEl as HTMLElement).style.display = "none";
    frameLimiter.reset();
    if (document.pointerLockElement === canvas) {
        document.exitPointerLock?.();
    }
    document.body.style.cursor = "default";
    windowFocused = true;
    pointerReleasedByEscape = false;
    setStatus("Stopped. You can restart or load a new ROM");
    updateRecOverlay();
    updateAutorunControls();
    updateEmulationButtons();
    updateSaveStateButtons();
}

/**
 * Trigger a browser download of an autorun recording.
 * @param {Uint8Array} bytes - The serialized AutorunFile JSON bytes.
 * @param {string|undefined} romName - ROM file name (used to derive the download file name).
 */
function triggerAutorunDownload(bytes: Uint8Array, romName: string | undefined) {
    const baseName = romName ? romName.replace(/\.nes$/i, "") : "recording";
    const fileName = `${baseName}.autorun`;
    const blob = new Blob([bytes as BlobPart], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = fileName;
    document.body.appendChild(anchor);
    anchor.click();
    document.body.removeChild(anchor);
    URL.revokeObjectURL(url);
    toastOverlay.show(`Autorun saved: ${fileName}`);
}

function startIdleScroller() {
    if (idleScrollerActive || romBytes) {
        return;
    }
    if (!webglInitialized && !initWebGL()) {
        setStatus("Failed to initialize WebGL", true);
        return;
    }
    if (!idleScroller) {
        idleScroller = createSineScroller({
            text: SCROLLER_TEXT,
            width,
            height,
            speed: SCROLLER_SPEED,
            amplitude: SCROLLER_AMPLITUDE,
            frequency: SCROLLER_FREQUENCY,
            fontSizePx: SCROLLER_FONT_SIZE_PX,
            fontFamily: SCROLLER_FONT_FAMILY
        });
    }
    idleScrollerActive = true;
    idleScrollerStartTime = 0;
    idleFrameLimiter.reset();
    requestAnimationFrame(stepIdleScroller);
}

function stopIdleScroller() {
    idleScrollerActive = false;
    idleScrollerStartTime = 0;
}

function stepIdleScroller(timestamp: number) {
    if (!idleScrollerActive || running || paused || romBytes) {
        return;
    }
    if (!idleFrameLimiter.shouldRender(timestamp)) {
        requestAnimationFrame(stepIdleScroller);
        return;
    }
    if (!idleScrollerStartTime) {
        idleScrollerStartTime = timestamp;
    }

    const frame = idleScroller!.renderFrame(timestamp);
    const rendered = renderFrameWithCurrentPipeline(frame);

    if (!rendered) {
        idleScrollerActive = false;
        setStatus("Rendering error occurred. Please restart.", true);
        return;
    }

    frameCount = (frameCount + 1) % 3600;
    requestAnimationFrame(stepIdleScroller);
}

function bindQuadAttributes(program: ShaderProgram) {
    if (program._aPositionLocation !== -1 && program._aPositionLocation !== null) {
        gl.bindBuffer(gl.ARRAY_BUFFER, positionBuffer);
        gl.enableVertexAttribArray(program._aPositionLocation as number);
        gl.vertexAttribPointer(program._aPositionLocation as number, 2, gl.FLOAT, false, 0, 0);
    }
    if (program._aTexCoordLocation !== -1 && program._aTexCoordLocation !== null) {
        gl.bindBuffer(gl.ARRAY_BUFFER, texCoordBuffer);
        gl.enableVertexAttribArray(program._aTexCoordLocation as number);
        gl.vertexAttribPointer(program._aTexCoordLocation as number, 2, gl.FLOAT, false, 0, 0);
    }
}

// ── GB 5-pass rendering pipeline ────────────────────────────────────────

/** Render a single-texture GB pass: bind input → run program → write to target FBO. */
function renderGbSimplePass(
    program: ShaderProgram,
    inputTex: WebGLTexture | null,
    targetFbo: WebGLFramebuffer | null,
    w: number, h: number,
    srcW: number, srcH: number,
) {
    gl.bindFramebuffer(gl.FRAMEBUFFER, targetFbo);
    gl.viewport(0, 0, w, h);
    gl.useProgram(program);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, inputTex);
    if (program._uTextureLocation != null)
        gl.uniform1i(program._uTextureLocation as WebGLUniformLocation, 0);
    if (program._uOutputSizeLocation)
        gl.uniform2f(program._uOutputSizeLocation as WebGLUniformLocation, w, h);
    if (program._uSourceSizeLocation)
        gl.uniform2f(program._uSourceSizeLocation as WebGLUniformLocation, srcW, srcH);
    bindQuadAttributes(program);
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
}

function renderGbPass(frame: Uint8Array): boolean {
    if (!gbPass0Program || !gbPass1Program || !gbPass2Program || !gbPass3Program || !gbPass4Program) {
        console.error("GB programs not initialized");
        return false;
    }
    // Assets (palette + background PNGs) load asynchronously; fall back to
    // a stock render until they are available so we don't show wrong colors.
    if (!gbAssetsLoaded || !gbPaletteTex || !gbBackgroundTex) {
        return renderSinglePass(frame) ?? false;
    }
    const cw = canvas.width;
    const ch = canvas.height;
    if (!ensureGbFbos(cw, ch)) return false;

    // Initialize previous frame data if needed
    if (!gbPrevFrameData || gbPrevFrameData.length !== frame.length) {
        gbPrevFrameData = new Uint8Array(frame.length);
    }

    // Upload current frame to nesTexture
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, nesTexture);
    gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, width, height, gl.RGBA, gl.UNSIGNED_BYTE, frame);

    // Upload previous frame to gbPrevFrameTex
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, gbPrevFrameTex);
    gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, width, height, gl.RGBA, gl.UNSIGNED_BYTE, gbPrevFrameData);

    // ── Pass 0: Dot-matrix + response time → FBO0 ──────────────────────
    gl.bindFramebuffer(gl.FRAMEBUFFER, gbFbo[0]);
    gl.viewport(0, 0, cw, ch);
    gl.useProgram(gbPass0Program);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, nesTexture);
    if (gbPass0Program._uTextureLocation != null)
        gl.uniform1i(gbPass0Program._uTextureLocation as WebGLUniformLocation, 0);
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, gbPrevFrameTex);
    if (gbPass0Program._uPrevFrameLocation != null)
        gl.uniform1i(gbPass0Program._uPrevFrameLocation as WebGLUniformLocation, 1);
    if (gbPaletteTex) {
        gl.activeTexture(gl.TEXTURE2);
        gl.bindTexture(gl.TEXTURE_2D, gbPaletteTex);
        if (gbPass0Program._uColorPaletteLocation != null)
            gl.uniform1i(gbPass0Program._uColorPaletteLocation as WebGLUniformLocation, 2);
    }
    if (gbPass0Program._uOutputSizeLocation)
        gl.uniform2f(gbPass0Program._uOutputSizeLocation as WebGLUniformLocation, cw, ch);
    if (gbPass0Program._uSourceSizeLocation)
        gl.uniform2f(gbPass0Program._uSourceSizeLocation as WebGLUniformLocation, width, height);
    bindQuadAttributes(gbPass0Program);
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);

    // ── Passes 1–3: alpha-blend → H-blur → V-blur (canvas→canvas) ─────
    renderGbSimplePass(gbPass1Program, gbTex[0], gbFbo[1], cw, ch, cw, ch);
    renderGbSimplePass(gbPass2Program, gbTex[1], gbFbo[2], cw, ch, cw, ch);
    renderGbSimplePass(gbPass3Program, gbTex[2], gbFbo[3], cw, ch, cw, ch);

    // ── Pass 4: Final compositing → screen ─────────────────────────────
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.viewport(0, 0, cw, ch);
    gl.useProgram(gbPass4Program);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, gbTex[3]);
    if (gbPass4Program._uTextureLocation != null)
        gl.uniform1i(gbPass4Program._uTextureLocation as WebGLUniformLocation, 0);
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, gbTex[1]);
    if (gbPass4Program._uGbPass1Location != null)
        gl.uniform1i(gbPass4Program._uGbPass1Location as WebGLUniformLocation, 1);
    if (gbBackgroundTex) {
        gl.activeTexture(gl.TEXTURE2);
        gl.bindTexture(gl.TEXTURE_2D, gbBackgroundTex);
        if (gbPass4Program._uBackgroundLocation != null)
            gl.uniform1i(gbPass4Program._uBackgroundLocation as WebGLUniformLocation, 2);
    }
    if (gbPaletteTex) {
        gl.activeTexture(gl.TEXTURE3);
        gl.bindTexture(gl.TEXTURE_2D, gbPaletteTex);
        if (gbPass4Program._uColorPaletteLocation != null)
            gl.uniform1i(gbPass4Program._uColorPaletteLocation as WebGLUniformLocation, 3);
    }
    if (gbPass4Program._uOutputSizeLocation)
        gl.uniform2f(gbPass4Program._uOutputSizeLocation as WebGLUniformLocation, cw, ch);
    if (gbPass4Program._uSourceSizeLocation)
        gl.uniform2f(gbPass4Program._uSourceSizeLocation as WebGLUniformLocation, cw, ch);
    if (gbPass4Program._uGbPass1SizeLocation)
        gl.uniform2f(gbPass4Program._uGbPass1SizeLocation as WebGLUniformLocation, cw, ch);
    bindQuadAttributes(gbPass4Program);
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);

    // Save current frame for next iteration's response time
    gbPrevFrameData!.set(frame);
    return true;
}

function renderSinglePass(frame: Uint8Array, sourceFormat = gl.RGBA) {
    if (!shaderProgram) {
        console.error("Shader program is null, cannot render");
        return false;
    }

    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, nesTexture);
    gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, width, height, sourceFormat, gl.UNSIGNED_BYTE, frame);

    gl.useProgram(shaderProgram);
    if (shaderProgram._uTextureSizeLocation) {
        gl.uniform2f(shaderProgram._uTextureSizeLocation, width, height);
    }
    if (shaderProgram._uSourceSizeLocation) {
        gl.uniform2f(shaderProgram._uSourceSizeLocation, width, height);
    }
    if (shaderProgram._uOutputSizeLocation) {
        gl.uniform2f(shaderProgram._uOutputSizeLocation, canvas.width, canvas.height);
    }
    if (shaderProgram._uFrameCountLocation) {
        gl.uniform1f(shaderProgram._uFrameCountLocation, frameCount);
    }
    if (shaderProgram._uTextureLocation) {
        gl.uniform1i(shaderProgram._uTextureLocation, 0);
    }

    const filter = filters[currentFilter];
    if (filter && filter.params) {
        const params = filter.params;
        if (shaderProgram._uHardScanLocation) gl.uniform1f(shaderProgram._uHardScanLocation, params.hardScan);
        if (shaderProgram._uHardPixLocation) gl.uniform1f(shaderProgram._uHardPixLocation, params.hardPix);
        if (shaderProgram._uWarpXLocation) gl.uniform1f(shaderProgram._uWarpXLocation, params.warpX);
        if (shaderProgram._uWarpYLocation) gl.uniform1f(shaderProgram._uWarpYLocation, params.warpY);
        if (shaderProgram._uMaskDarkLocation) gl.uniform1f(shaderProgram._uMaskDarkLocation, params.maskDark);
        if (shaderProgram._uMaskLightLocation) gl.uniform1f(shaderProgram._uMaskLightLocation, params.maskLight);
        if (shaderProgram._uScaleInLinearGammaLocation) gl.uniform1f(shaderProgram._uScaleInLinearGammaLocation, params.scaleInLinearGamma);
        if (shaderProgram._uShadowMaskLocation) gl.uniform1f(shaderProgram._uShadowMaskLocation, params.shadowMask);
        if (shaderProgram._uBrightBoostLocation) gl.uniform1f(shaderProgram._uBrightBoostLocation, params.brightBoost);
        if (shaderProgram._uHardBloomScanLocation) gl.uniform1f(shaderProgram._uHardBloomScanLocation, params.hardBloomScan);
        if (shaderProgram._uHardBloomPixLocation) gl.uniform1f(shaderProgram._uHardBloomPixLocation, params.hardBloomPix);
        if (shaderProgram._uBloomAmountLocation) gl.uniform1f(shaderProgram._uBloomAmountLocation, params.bloomAmount);
        if (shaderProgram._uShapeLocation) gl.uniform1f(shaderProgram._uShapeLocation, params.shape);
    }

    bindQuadAttributes(shaderProgram);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.viewport(0, 0, canvas.width, canvas.height);
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
    return true;
}

function renderNtscPass(frame: Uint8Array) {
    if (!ntscPass1Program || !ntscPass2Program || !ntscPass1Framebuffer || !ntscPass1Texture) {
        console.error("NTSC programs or targets are not initialized");
        return false;
    }

    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, nesTexture);
    gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, width, height, gl.RGBA, gl.UNSIGNED_BYTE, frame);

    // Pass 1: encode/demodulate to YIQ
    gl.bindFramebuffer(gl.FRAMEBUFFER, ntscPass1Framebuffer);
    gl.viewport(0, 0, ntscPass1Width, ntscPass1Height);
    gl.useProgram(ntscPass1Program);
    if (ntscPass1Program._uOutputSizeLocation) {
        gl.uniform2f(ntscPass1Program._uOutputSizeLocation, ntscPass1Width, ntscPass1Height);
    }
    if (ntscPass1Program._uFrameCountLocation) {
        gl.uniform1f(ntscPass1Program._uFrameCountLocation, frameCount % 2);
    }
    if (ntscPass1Program._uChromaEncodeLocation) {
        gl.uniform1f(ntscPass1Program._uChromaEncodeLocation, ntscChromaEncode);
    }
    if (ntscPass1Program._uTextureLocation) {
        gl.uniform1i(ntscPass1Program._uTextureLocation, 0);
    }
    bindQuadAttributes(ntscPass1Program);
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);

    // Pass 2: decode/filter to RGB
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.bindTexture(gl.TEXTURE_2D, ntscPass1Texture);
    gl.viewport(0, 0, canvas.width, canvas.height);
    gl.useProgram(ntscPass2Program);
    if (ntscPass2Program._uSourceSizeLocation) {
        gl.uniform2f(ntscPass2Program._uSourceSizeLocation, ntscPass1Width, ntscPass1Height);
    }
    if (ntscPass2Program._uChromaEncodeLocation) {
        gl.uniform1f(ntscPass2Program._uChromaEncodeLocation, ntscChromaEncode);
    }
    if (ntscPass2Program._uChromaSumLocation) {
        gl.uniform1f(ntscPass2Program._uChromaSumLocation, ntscChromaSum);
    }
    if (ntscPass2Program._uTextureLocation) {
        gl.uniform1i(ntscPass2Program._uTextureLocation, 0);
    }
    bindQuadAttributes(ntscPass2Program);
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
    return true;
}

function step(timestamp: number) {
    if (!running || paused) return;
    lastFrameTime = timestamp;
    const { shouldStep, shouldRender } = planFrame({
        shouldRender: frameLimiter.shouldRender(timestamp)
    });
    try {
        if (emulator) {
            pollGamepad();
        }

        if (!shouldStep) {
            requestAnimationFrame(step);
            return;
        }

        const wasmT0 = performance.now();
        const frame = emulator!.kind === "gba"
            ? emulator!.inst.render_frame_rgb()
            : emulator!.inst.render_frame_rgba();
        const sourceFormat = frameTextureFormat();
        const wasmElapsedMs = performance.now() - wasmT0;

        // Stop when pure NES autorun playback has consumed all recorded frames
        if (nes?.autorun_playback_finished()) {
            stop();
            setStatus("Autorun playback complete.");
            return;
        }

        let rendered = true;
        const renderT0 = performance.now();
        if (shouldRender) {
            rendered = renderFrameWithCurrentPipeline(frame, sourceFormat);
        }
        const renderElapsedMs = performance.now() - renderT0;

        if (!rendered) {
            running = false;
            setStatus("Rendering error occurred. Please restart.", true);
            return;
        }

        // Increment frame counter for NTSC phase animation
        // Wrap at 3600 to prevent float precision issues (60 frames/sec * 60 sec = 3600 frames/min)
        frameCount = (frameCount + 1) % 3600;

        // Get and play audio samples
        const audio = getPlaybackAudioSamples(emulator!.kind, emulator!.inst);
        if (audio.samples.length > 0) {
            playAudioSamples(audio.samples, audio.channels);
        }

        updateRecOverlay();

        fpsFrames += 1;
        fpsWasmTimeAccMs += wasmElapsedMs;
        fpsRenderTimeAccMs += renderElapsedMs;
        if (fpsLastTime === 0) {
            fpsLastTime = timestamp;
        }
        const fpsElapsed = timestamp - fpsLastTime;
        if (fpsElapsed >= fpsLogIntervalMs) {
            const fps = (fpsFrames * 1000) / fpsElapsed;
            const avgWasm = fpsWasmTimeAccMs / fpsFrames;
            const avgRender = fpsRenderTimeAccMs / fpsFrames;
            const msg = `${fps.toFixed(1)} fps | emu ${avgWasm.toFixed(1)}ms | gl ${avgRender.toFixed(1)}ms`;
            console.log(msg);
            if (fpsCounterEl) {
                fpsCounterEl.textContent = msg;
                (fpsCounterEl as HTMLElement).style.display = "";
            }
            fpsFrames = 0;
            fpsLastTime = timestamp;
            fpsWasmTimeAccMs = 0;
            fpsRenderTimeAccMs = 0;
        }
    } catch (err) {
        running = false;
        paused = false;
        updateEmulationButtons();
        romInput.disabled = false;
        setStatus(`Emulation error: ${err}`, true);
        if (console && typeof console.error === "function") {
            console.error("Emulation error during render_frame", err);
        }
    }
    if (running) {
        requestAnimationFrame(step);
    }
}

startBtn.addEventListener("click", () => {
    requestPointerLockFromUserGesture();
    void start();
});
const muteBtn = document.getElementById("mute") as HTMLButtonElement | null;
function updateMuteButton() {
    muteBtn!.textContent = audioMuted ? "Audio: Off" : "Audio: On";
    muteBtn!.setAttribute("aria-pressed", audioMuted ? "true" : "false");
}
muteBtn!.addEventListener("click", async () => {
    audioMuted = !audioMuted;
    updateMuteButton();
    if (emulator) {
        emulator.inst.set_audio_muted(audioMuted);
    }
    if (audioContext) {
        try {
            if (audioMuted && audioContext.state === "running") {
                await audioContext.suspend();
            } else if (!audioMuted && audioContext.state === "suspended") {
                nextAudioTime = audioContext.currentTime;
                await audioContext.resume();
            }
        } catch (err) {
            console.error("Failed to toggle audio context state:", err);
        }
    }
});
updateMuteButton();
const pauseBtn = document.getElementById("pause") as HTMLButtonElement | null;
const stopBtn = document.getElementById("stop") as HTMLButtonElement | null;
const resetBtn = document.getElementById("reset") as HTMLButtonElement | null;
if (!pauseBtn || !stopBtn || !resetBtn) {
    throw new Error("Pause/Stop/Reset buttons not found in DOM");
}
pauseBtn.addEventListener("click", pauseResume);
stopBtn.addEventListener("click", stop);
resetBtn.addEventListener("click", () => {
    void resetAction();
});

async function populateRomSelect() {
    if (!romSelect) return;
    const baseUrl = new URL("./roms/", window.location.href).toString();
    try {
        const entries = await fetchRomList(baseUrl);
        for (const entry of entries) {
            const option = document.createElement("option");
            option.value = entry.url;
            option.textContent = entry.path;
            romSelect.appendChild(option);
        }
    } catch (error) {
        console.error("Failed to load ROM list", error);
    }
}

populateRomSelect();
// Set initial button states (all disabled until a ROM is loaded)
updateEmulationButtons();

// Keyboard input mappings for both controllers
// Controller 1: W=Up, S=Down, A=Left, D=Right, R=B, T=A, Y=X, G=Y, Q=L, E=R, 4=Select, 5=Start
const keyToButtonController1: Record<string, { button?: number; snesButton?: number; name: string }> = {
    'w': { button: 4, snesButton: 4, name: 'Up' },        // NES Up / SNES Up
    's': { button: 5, snesButton: 5, name: 'Down' },      // NES Down / SNES Down
    'a': { button: 6, snesButton: 6, name: 'Left' },      // NES Left / SNES Left
    'd': { button: 7, snesButton: 7, name: 'Right' },     // NES Right / SNES Right
    'r': { button: 0, snesButton: 0, name: 'B' },         // NES A fallback / SNES B
    't': { button: 1, snesButton: 8, name: 'A' },         // NES B fallback / SNES A
    'y': { snesButton: 9, name: 'X' },                    // SNES X only
    'g': { snesButton: 1, name: 'Y' },                    // SNES Y only
    'q': { snesButton: 10, name: 'L' },                   // SNES L only
    'e': { snesButton: 11, name: 'R' },                   // SNES R only
    '4': { button: 2, snesButton: 2, name: 'Select' },    // NES Select / SNES Select
    '5': { button: 3, snesButton: 3, name: 'Start' }      // NES Start / SNES Start
};

// Controller 2: I=Up, K=Down, J=Left, L=Right, P=B, O=A, 9=Select, 0=Start
const keyToButtonController2: Record<string, { button?: number; snesButton?: number; name: string }> = {
    'i': { button: 4, name: 'Up' },      // Button 4 = Up
    'k': { button: 5, name: 'Down' },    // Button 5 = Down
    'j': { button: 6, name: 'Left' },    // Button 6 = Left
    'l': { button: 7, name: 'Right' },   // Button 7 = Right
    'p': { button: 1, name: 'B' },       // Button 1 = B
    'o': { button: 0, name: 'A' },       // Button 0 = A
    '9': { button: 2, name: 'Select' },  // Button 2 = Select
    '0': { button: 3, name: 'Start' }    // Button 3 = Start
};

// Track connected gamepads for routing
let connectedGamepads: Gamepad[] = [];

function updateConnectedGamepads() {
    const gamepads = navigator.getGamepads ? navigator.getGamepads() : [];
    connectedGamepads = selectGamepads(gamepads);
    return connectedGamepads;
}

function showPageLoadGamepadInitToast() {
    toastOverlay.show("Press a button on any connected gamepad");
}

// Initialize connectedGamepads to detect any gamepads already connected on page load
updateConnectedGamepads();

ensureWasmInitialized()
    .then(() => {
        updateConnectedGamepads();
        showPageLoadGamepadInitToast();
    })
    .catch((error) => {
        console.error("Failed to initialize WASM for gamepad init toast", error);
    });

const webShortcutActions = {
    togglePause: pauseResume,
    reset: resetAction,
    hardReset: hardResetAction,
    toggleFilter: toggleFilterAction,
    saveState: saveStateAction,
    loadState: loadStateAction,
    toggleFullscreen: toggleScreenFullscreen,
    toggleHelp: toggleShortcutHelp,
    debuggerToggle,
    debuggerStepOver,
    debuggerStepInto,
    cyclePalette: cyclePaletteAction,
};

function updateShortcutHelpOverlayText() {
    if (shortcutHelpOverlay) {
        shortcutHelpOverlay.textContent = buildFullHelpOverlayText(
            connectedGamepads.length,
            emulator?.kind ?? "nes"
        );
    }
}

function toggleShortcutHelp() {
    updateShortcutHelpOverlayText();
    toggleShortcutHelpVisibility(shortcutHelpOverlay);
}

function updateFilterToggleButtonLabel() {
    filterToggleBtn.textContent = `Filter: ${filters[currentFilter].name}`;
}

function toggleFilterAction() {
    cycleFilter();
    updateFilterToggleButtonLabel();
}

function applyKeyboardMapping(event: KeyboardEvent, mapping: { button?: number; snesButton?: number; name: string } | undefined, controller: number, targets: number[], pressed: boolean) {
    if (!mapping || !targets.includes(controller)) {
        return;
    }
    event.preventDefault();

    // Try SNES button mapping first when available.
    if (nes && mapping.snesButton !== undefined) {
        const handledAsSnes = nes.set_snes_button(controller, mapping.snesButton, pressed);
        if (handledAsSnes) {
            return;
        }
    }
    if (emulator?.kind === "snes" && mapping.snesButton !== undefined) {
        emulator.inst.set_button(controller, remapLegacySnesButtonId(mapping.snesButton), pressed);
        return;
    }

    if (mapping.button !== undefined) {
        if (nes) {
            applyJoypadButtonIfAllowed(nes, controller, mapping.button, pressed);
        } else if (emulator) {
            // GB: direct button routing (no mouse/zapper suppression needed).
            emulator.inst.set_button(controller, mapping.button, pressed);
        }
    }
}

function isEditableKeyboardTarget(target: EventTarget | null) {
    if (!(target instanceof HTMLElement)) {
        return false;
    }
    const tag = target.tagName.toUpperCase();
    return target.isContentEditable || tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
}

async function handleKeyDown(event: KeyboardEvent) {
    if (isEditableKeyboardTarget(event.target)) {
        return;
    }

    if (event.key === "Escape") {
        pointerReleasedByEscape = true;
        if (document.pointerLockElement === canvas) {
            document.exitPointerLock?.();
        }
        updateMouseCursorState();
        return;
    }

    if (!emulator && event.code !== "KeyH") {
        return;
    }

    const handledShortcut = await dispatchWebShortcutAction(event, webShortcutActions);
    if (handledShortcut) {
        return;
    }

    if (!emulator) {
        return;
    }

    // NES-only: block keyboard input while debugger is open.
    if (nes?.is_debugger_open()) {
        return;
    }

    if (emulator.kind === "gba") {
        const button = gbaKeyboardButtonForEvent(event);
        if (button !== null) {
            event.preventDefault();
            emulator.inst.set_button(1, button, true);
        }
        return;
    }

    const key = event.key.toLowerCase();
    const targets = getKeyboardControllerTarget(
        connectedGamepads.length,
        nes?.is_four_score_enabled?.() ?? false
    );

    applyKeyboardMapping(event, keyToButtonController1[key], targets[0] ?? 1, targets, true);
    applyKeyboardMapping(event, keyToButtonController2[key], targets[1] ?? 2, targets, true);
}

function handleKeyUp(event: KeyboardEvent) {
    if (isEditableKeyboardTarget(event.target)) {
        return;
    }

    if (!emulator) {
        return;
    }

    // NES-only: block keyboard input while debugger is open.
    if (nes?.is_debugger_open()) {
        return;
    }

    if (emulator.kind === "gba") {
        const button = gbaKeyboardButtonForEvent(event);
        if (button !== null) {
            event.preventDefault();
            emulator.inst.set_button(1, button, false);
        }
        return;
    }

    const key = event.key.toLowerCase();
    const targets = getKeyboardControllerTarget(
        connectedGamepads.length,
        nes?.is_four_score_enabled?.() ?? false
    );

    applyKeyboardMapping(event, keyToButtonController1[key], targets[0] ?? 1, targets, false);
    applyKeyboardMapping(event, keyToButtonController2[key], targets[1] ?? 2, targets, false);
}

document.addEventListener('keydown', handleKeyDown);
document.addEventListener('keyup', handleKeyUp);

// ── Touch controls ──────────────────────────────────────────────────────────
if (isTouchDevice()) {
    document.body.classList.add("touch-device");
}
if (isHandheldDevice()) {
    document.body.classList.add("handheld");
}

const touchControlsContainer = document.getElementById("touch-controls");

function handleTouchButton(button: number, pressed: boolean) {
    if (!emulator) return;
    if (nes) {
        applyJoypadButtonIfAllowed(nes, 1, button, pressed);
    } else {
        emulator.inst.set_button(1, button, pressed);
    }
}

if (touchControlsContainer) {
    initTouchControls(touchControlsContainer, handleTouchButton);
}

function handleMouseMotion(event: MouseEvent) {
    if (!nes) return;

    const mouseControllerActive = isMouseControllerActive(nes);
    const pointerLocked = document.pointerLockElement === canvas;
    if (mouseControllerActive && !shouldForwardArkanoidMouseInput({ pointerLocked })) {
        return;
    }

    const rect = canvas.getBoundingClientRect();
    if (rect.width <= 1 || rect.height <= 1) {
        return;
    }

    let x = event.clientX - rect.left;
    let y = event.clientY - rect.top;

    if (pointerLocked) {
        const maxX = Math.max(0, rect.width - 1);
        const maxY = Math.max(0, rect.height - 1);
        lockedPointerX = Math.min(maxX, Math.max(0, lockedPointerX + event.movementX));
        lockedPointerY = Math.min(maxY, Math.max(0, lockedPointerY + event.movementY));
        x = lockedPointerX;
        y = lockedPointerY;
    } else {
        lockedPointerX = x;
        lockedPointerY = y;
    }

    applyMouseMotion(nes, x, y, rect.width, rect.height);
    
    // Update crosshair position if visible
    if (crosshair && crosshair.visible) {
        crosshair.updatePosition(x, y);
    }
}

function isArkanoidControllerActive(nesInstance: WasmNes | null) {
    if (!nesInstance) {
        return false;
    }

    const mouseOnAnyPort =
        nesInstance.is_mouse_emulated_controller(1) ||
        nesInstance.is_mouse_emulated_controller(2) ||
        nesInstance.has_expansion_mouse_controller();
    return mouseOnAnyPort && !isZapperActive(nesInstance);
}

function isMouseControllerActive(nesInstance: WasmNes | null) {
    if (!nesInstance) {
        return false;
    }

    return (
        nesInstance.is_mouse_emulated_controller(1) ||
        nesInstance.is_mouse_emulated_controller(2) ||
        nesInstance.has_expansion_mouse_controller()
    );
}

function setCrosshairVisible(visible: boolean) {
    if (visible) {
        if (!crosshair) {
            crosshair = createCrosshair(canvas);
        }
        crosshair.show();
        return;
    }

    if (crosshair) {
        crosshair.destroy();
        crosshair = null;
    }
}

function updateMouseCursorState() {
    if (!nes) {
        // No NES active (GB or no emulator): ensure any NES-specific cursor state is cleared.
        setCrosshairVisible(false);
        if (document.pointerLockElement === canvas) {
            document.exitPointerLock?.();
        }
        document.body.style.cursor = "";
        return;
    }

    const zapperActive = isZapperActive(nes);
    const pointerLocked = document.pointerLockElement === canvas;
    setCrosshairVisible(zapperActive && pointerLocked);

    if (zapperActive && pointerLocked) {
        document.body.style.cursor = "none";
        return;
    }

    const arkanoidActive = isArkanoidControllerActive(nes);

    const keepPointerLocked = shouldKeepPointerLocked({
        arkanoidActive,
        windowFocused,
        releasedByEscape: pointerReleasedByEscape,
    });

    if (!keepPointerLocked && document.pointerLockElement === canvas) {
        document.exitPointerLock?.();
    }

    document.body.style.cursor = computeMouseCursorStyle({
        arkanoidActive,
        windowFocused,
        releasedByEscape: pointerReleasedByEscape,
    });
}

function handleMouseButton(event: MouseEvent, pressed: boolean) {
    if (!nes) return;

    const mouseControllerActive = isMouseControllerActive(nes);
    const pointerLocked = document.pointerLockElement === canvas;
    if (mouseControllerActive && !shouldForwardArkanoidMouseInput({ pointerLocked })) {
        return;
    }

    applyMouseButton(nes, event.button, pressed);
}

window.addEventListener("mousemove", handleMouseMotion);
canvas.addEventListener("mousedown", (event) => {
    pointerReleasedByEscape = false;
    requestPointerLockFromUserGesture();
    updateMouseCursorState();
    handleMouseButton(event, true);
});
window.addEventListener("mouseup", (event) => handleMouseButton(event, false));
window.addEventListener("focus", () => {
    windowFocused = true;
    updateMouseCursorState();
});
window.addEventListener("blur", () => {
    windowFocused = false;
    pointerReleasedByEscape = true;
    updateMouseCursorState();
});
document.addEventListener("pointerlockchange", () => {
    if (document.pointerLockElement === canvas) {
        const rect = canvas.getBoundingClientRect();
        lockedPointerX = rect.width * 0.5;
        lockedPointerY = rect.height * 0.5;
    } else {
        pointerReleasedByEscape = true;
    }
    updateMouseCursorState();
});

// Screen size controls
const screenMinusBtn = document.getElementById("screen-minus") as HTMLButtonElement;
const screenPlusBtn = document.getElementById("screen-plus") as HTMLButtonElement;
const fullscreenBtn = document.getElementById("fullscreen") as HTMLButtonElement;
const filterToggleBtn = document.getElementById("filter-toggle") as HTMLButtonElement;
const saveStateBtn = document.getElementById("save-state") as HTMLButtonElement | null;
const loadStateBtn = document.getElementById("load-state") as HTMLButtonElement | null;

// NES native resolution is 256x240 pixels; aspect ratio updated after NES init.
let NES_ASPECT_RATIO = width / height;
const SCALE_STEP = 120; // Change height by 120px each step
const INITIAL_HEIGHT = 720; // Initial display height in pixels
let currentHeight = INITIAL_HEIGHT;

function applyCanvasSize(size: { cssWidth: string; cssHeight: string; pixelWidth: number; pixelHeight: number }) {
    canvas.style.width = size.cssWidth;
    canvas.style.height = size.cssHeight;
    canvas.width = size.pixelWidth;
    canvas.height = size.pixelHeight;
}

function updateCanvasSize(newHeight: number) {
    const dpr = window.devicePixelRatio || 1;
    const size = computeWindowedCanvasSize(newHeight, NES_ASPECT_RATIO, dpr);
    currentHeight = size.pixelHeight / dpr; // track the clamped height
    applyCanvasSize(size);
    if (crosshair) {
        crosshair.updateCanvasSize();
    }
    updateShortcutHelpScale();
}

function updateCanvasSizeForFullscreenViewport() {
    const dpr = window.devicePixelRatio || 1;
    const size = computeFullscreenCanvasSize(window.innerWidth, window.innerHeight, NES_ASPECT_RATIO, dpr);
    applyCanvasSize(size);
    if (crosshair) {
        crosshair.updateCanvasSize();
    }
    updateShortcutHelpScale();
}

function updateHandheldOrientation() {
    const isPortrait = window.matchMedia("(orientation: portrait)").matches;
    document.body.classList.toggle("handheld-portrait", isPortrait);
}

function updateHandheldCanvasSize() {
    const dpr = window.devicePixelRatio || 1;
    const isPortrait = window.matchMedia("(orientation: portrait)").matches;
    const size = computeHandheldCanvasSize(isPortrait, window.innerWidth, window.innerHeight, NES_ASPECT_RATIO, dpr);
    applyCanvasSize(size);
    if (crosshair) {
        crosshair.updateCanvasSize();
    }
    updateShortcutHelpScale();
}

function resizeCanvasForCurrentDisplayMode() {
    if (isInFullscreen() && !isHandheldDevice()) {
        updateCanvasSizeForFullscreenViewport();
    } else if (isHandheldDevice()) {
        updateHandheldCanvasSize();
    } else {
        updateCanvasSize(currentHeight);
    }
}

function updateShortcutHelpScale() {
    if (!shortcutHelpOverlay) {
        return;
    }
    const fontSizePx = computeShortcutHelpFontSizePx(canvas.clientHeight);
    shortcutHelpOverlay.style.fontSize = `${fontSizePx}px`;
}

function measureDisplayHeightAt(height: number) {
    updateCanvasSize(height);
    return canvas.clientHeight;
}

function probeNextVisibleZoomHeight(direction: "in" | "out") {
    const startHeight = currentHeight;
    const nextHeight = findNextVisibleZoomHeight({
        direction,
        currentHeight: startHeight,
        step: SCALE_STEP,
        measureDisplayHeight: measureDisplayHeightAt,
    });

    if (nextHeight === null) {
        updateCanvasSize(startHeight);
    }

    return nextHeight;
}

/** Returns the element currently used as the fullscreen root (may be screenWrap or documentElement on handheld). */
function fullscreenRoot(): HTMLElement {
    return isHandheldDevice() ? document.documentElement : screenWrap;
}

function isInFullscreen(): boolean {
    return document.fullscreenElement === fullscreenRoot();
}

function updateZoomButtonState() {
    const inScreenFullscreen = isInFullscreen();
    if (inScreenFullscreen) {
        screenMinusBtn.disabled = true;
        screenPlusBtn.disabled = true;
        return;
    }

    const startHeight = currentHeight;
    const canZoomOut = probeNextVisibleZoomHeight("out") !== null;
    updateCanvasSize(startHeight);
    const canZoomIn = probeNextVisibleZoomHeight("in") !== null;
    updateCanvasSize(startHeight);

    screenMinusBtn.disabled = !canZoomOut;
    screenPlusBtn.disabled = !canZoomIn;
}

function applyZoom(direction: "in" | "out") {
    if (isInFullscreen()) {
        updateZoomButtonState();
        return;
    }

    const startHeight = currentHeight;
    const nextHeight = probeNextVisibleZoomHeight(direction);
    if (nextHeight === null) {
        updateCanvasSize(startHeight);
        updateZoomButtonState();
        return;
    }

    updateCanvasSize(nextHeight);

    updateZoomButtonState();
}

// Update fullscreen button text based on state
function updateFullscreenButton() {
    fullscreenBtn.textContent = isInFullscreen() ? "Exit Fullscreen" : "Fullscreen";
}

async function toggleScreenFullscreen() {
    if (!isInFullscreen()) {
        try {
            await fullscreenRoot().requestFullscreen();
        } catch (err) {
            console.error("Failed to enter fullscreen:", err);
            setStatus("Failed to enter fullscreen mode", true);
        }
    } else {
        try {
            await document.exitFullscreen();
        } catch (err) {
            console.error("Failed to exit fullscreen:", err);
            setStatus("Failed to exit fullscreen mode", true);
        }
    }
}

async function resetAction() {
    if (!emulator) return;
    cancelActiveRecording();
    if (emulator.kind === "gba") {
        await restartGbaSession("Soft reset");
        return;
    }
    emulator.inst.reset(true);
    setStatus("Soft reset", false);
}

async function hardResetAction() {
    if (!emulator) return;
    cancelActiveRecording();
    if (emulator.kind === "gba") {
        await restartGbaSession("Hard reset");
        return;
    }
    emulator.inst.reset(false);
    setStatus("Hard reset", false);
}

async function restartGbaSession(status: string) {
    if (!romBytes || !romMetadata) return;
    const shouldScheduleFrameLoop = !running || paused;
    try {
        createEmulatorInstance("gba");
        emulator!.inst.load_rom(romBytes, romMetadata.name);
        drainNesToasts(emulator?.inst ?? null, toastOverlay);
        frameLimiter.setTargetFps(emulator!.inst.frame_rate_hz());
        await initAudioContext();
        if (audioContext) nextAudioTime = audioContext.currentTime;
        configureActiveEmulatorAudioSampleRate();
        emulator!.inst.set_audio_muted(audioMuted);
        await refreshSaveStateController();
        running = true;
        paused = false;
        lastFrameTime = 0;
        fpsFrames = 0;
        fpsLastTime = 0;
        fpsWasmTimeAccMs = 0;
        fpsRenderTimeAccMs = 0;
        frameLimiter.reset();
        if (isTouchDevice()) document.body.classList.add("touch-running");
        if (isHandheldDevice()) updateHandheldCanvasSize();
        const sidebarToggle = document.getElementById("sidebar-toggle") as HTMLInputElement | null;
        if (sidebarToggle) sidebarToggle.checked = false;
        setStatus(status, false);
        updateMouseCursorState();
        updateAutorunControls();
        updateEmulationButtons();
        updateSaveStateButtons();
        if (shouldScheduleFrameLoop) {
            requestAnimationFrame(step);
        }
    } catch (err) {
        drainNesToasts(emulator?.inst ?? null, toastOverlay);
        running = false;
        paused = false;
        setStatus(`Failed to reset GBA: ${err}`, true);
        updateEmulationButtons();
    }
}

/** Cancel an active autorun recording (if any), updating UI and showing a toast. */
function cancelActiveRecording() {
    if (!nes || !nes.autorun_is_recording()) return;
    nes.clear_autorun();
    autorunCtx.setCreateRecording(false);
    if (autorunCreateCheckbox) autorunCreateCheckbox.checked = false;
    updateRecOverlay();
    updateAutorunControls();
    updateEmulationButtons();
    toastOverlay.show("Recording cancelled");
}

async function saveStateAction() {
    if (!saveStateController) return;
    const ok = await saveStateController.save();
    if (ok) {
        saveStateAvailable = true;
        updateSaveStateButtons();
    }
}

async function loadStateAction() {
    if (!saveStateController) return;
    await saveStateController.load();
}

function updateSaveStateButtons() {
    const enabled = Boolean(saveStateController) && running;
    if (saveStateBtn) saveStateBtn.disabled = !enabled;
    if (loadStateBtn) loadStateBtn.disabled = !enabled || !saveStateAvailable;
}

// Set initial canvas size and button text
if (isHandheldDevice()) {
    updateHandheldOrientation();
    updateHandheldCanvasSize();
} else {
    updateCanvasSize(INITIAL_HEIGHT);
}
updateFullscreenButton();
updateZoomButtonState();
updateFilterToggleButtonLabel();
updateSaveStateButtons();
const shortcutReferenceText = buildShortcutReferenceText();
if (shortcutReference) {
    shortcutReference.textContent = `Shortcuts: ${shortcutReferenceText}`;
}
if (shortcutHelpOverlay) {
    updateShortcutHelpOverlayText();
}
updateShortcutHelpScale();
startIdleScroller();

screenMinusBtn.addEventListener("click", () => {
    applyZoom("out");
});

screenPlusBtn.addEventListener("click", () => {
    applyZoom("in");
});

fullscreenBtn.addEventListener("click", async () => {
    await toggleScreenFullscreen();
});

filterToggleBtn.addEventListener("click", toggleFilterAction);

saveStateBtn?.addEventListener("click", async () => {
    await saveStateAction();
});

loadStateBtn?.addEventListener("click", async () => {
    await loadStateAction();
});

function pollGamepad() {
    const gamepads = navigator.getGamepads ? navigator.getGamepads() : [];
    connectedGamepads = selectGamepads(gamepads);
    
    // Apply first gamepad to controller 1
    if (connectedGamepads.length >= 1) {
        const state1 = mapStandardGamepadState(connectedGamepads[0]);
        applyGamepadState(state1, 1, lastGamepadState1);
        lastGamepadState1 = state1;
    }
    
    // Apply second gamepad to controller 2
    if (connectedGamepads.length >= 2) {
        const state2 = mapStandardGamepadState(connectedGamepads[1]);
        applyGamepadState(state2, 2, lastGamepadState2);
        lastGamepadState2 = state2;
    }
}

interface GamepadButtonState {
    a: boolean;
    b: boolean;
    x: boolean;
    y: boolean;
    select: boolean;
    start: boolean;
    up: boolean;
    down: boolean;
    left: boolean;
    right: boolean;
    l: boolean;
    r: boolean;
}

function applyGamepadState(state: GamepadButtonState, controller: number, lastState: GamepadButtonState) {
    if (!emulator) return;
    if (emulator.kind === "gba" && controller !== 1) return;
    const applyButton = (button: number, pressed: boolean) => {
        if (nes) {
            applyJoypadButtonIfAllowed(nes, controller, button, pressed);
        } else {
            emulator!.inst.set_button(controller, button, pressed);
        }
    };
    const applySnesButton = (
        changed: boolean,
        pressed: boolean,
        legacyButton: number,
        snesCoreButton: number,
        fallbackNesButton?: number
    ) => {
        if (!changed) return;

        if (nes?.set_snes_button(controller, legacyButton, pressed)) {
            return;
        }

        if (emulator!.kind === "snes") {
            emulator!.inst.set_button(controller, snesCoreButton, pressed);
            return;
        }

        if (fallbackNesButton !== undefined) {
            applyButton(fallbackNesButton, pressed);
        }
    };

    applySnesButton(state.a !== lastState.a, state.a, 0, 1, 0); // South -> SNES B, NES A
    applySnesButton(state.b !== lastState.b, state.b, 8, 0, 1); // East -> SNES A, NES B
    applySnesButton(state.y !== lastState.y, state.y, 1, 11); // West -> SNES Y
    applySnesButton(state.x !== lastState.x, state.x, 9, 10); // North -> SNES X
    applySnesButton(state.select !== lastState.select, state.select, 2, 2, 2);
    applySnesButton(state.start !== lastState.start, state.start, 3, 3, 3);
    applySnesButton(state.up !== lastState.up, state.up, 4, 4, 4);
    applySnesButton(state.down !== lastState.down, state.down, 5, 5, 5);
    applySnesButton(state.left !== lastState.left, state.left, 6, 6, 6);
    applySnesButton(state.right !== lastState.right, state.right, 7, 7, 7);

    if (emulator.kind === "gba") {
        if (state.l !== lastState.l) applyButton(8, state.l);
        if (state.r !== lastState.r) applyButton(9, state.r);
    } else {
        applySnesButton(state.l !== lastState.l, state.l, 10, 8);
        applySnesButton(state.r !== lastState.r, state.r, 11, 9);
    }
}

function resetGamepadState() {
    const emptyState = {
        a: false,
        b: false,
        x: false,
        y: false,
        select: false,
        start: false,
        up: false,
        down: false,
        left: false,
        right: false,
        l: false,
        r: false
    };
    applyGamepadState(emptyState, 1, lastGamepadState1);
    applyGamepadState(emptyState, 2, lastGamepadState2);
    lastGamepadState1 = { ...emptyState };
    lastGamepadState2 = { ...emptyState };
}

function onGamepadConnectionChanged() {
    updateConnectedGamepads();
    updateShortcutHelpOverlayText();
    ensureWasmInitialized()
        .then(() => toastOverlay.show(gamepad_init_toast_message(true, connectedGamepads.length)))
        .catch(() => {});
}

window.addEventListener("gamepadconnected", () => {
    onGamepadConnectionChanged();
    if (running && !paused) {
        pollGamepad();
    }
});

window.addEventListener("gamepaddisconnected", () => {
    onGamepadConnectionChanged();
    resetGamepadState();
});

// Handle canvas resizing when entering/exiting fullscreen
document.addEventListener("fullscreenchange", () => {
    updateFullscreenButton();
    if (isInFullscreen()) {
        if (!isHandheldDevice()) {
            updateCanvasSizeForFullscreenViewport();
        }
        // Handheld fullscreen keeps the normal handheld layout; no canvas resize needed here.
    } else {
        if (isHandheldDevice()) {
            updateHandheldCanvasSize();
        } else {
            // Exited fullscreen - restore previous size
            updateCanvasSize(currentHeight);
        }
    }
    updateZoomButtonState();
});

// Re-fit canvas when viewport resizes while in fullscreen (e.g. orientation change)
window.addEventListener("resize", () => {
    if (isInFullscreen() && !isHandheldDevice()) {
        updateCanvasSizeForFullscreenViewport();
        updateZoomButtonState();
        return;
    }

    if (isHandheldDevice()) {
        updateHandheldOrientation();
        updateHandheldCanvasSize();
    }

    updateZoomButtonState();
});

window.addEventListener("orientationchange", () => {
    if (isHandheldDevice()) {
        updateHandheldOrientation();
        updateHandheldCanvasSize();
    }
});
