import init, { WasmNes, gamepad_init_toast_message } from "./pkg/neser.js?v=20260127";
import { mapStandardGamepadState, selectGamepads } from "./gamepad.js";
import {
    createRomSaveKey,
    hasState,
    loadState,
    openSaveStateDb,
    saveState
} from "./save_state_storage.js";
import { createSaveStateController } from "./save_state_controller.js";
import { applyJoypadButtonIfAllowed, applyMouseMotion, applyMouseButton, isZapperActive } from "./mouse_input.js";
import { createSaveStateContext } from "./save_state_context.js";
import { fetchRomList } from "./rom_list.js";
import { handleRomSelection } from "./rom_selection.js";
import { createAutorunContext, parseAutorunFile } from "./autorun_context.js";
import { createFrameLimiter } from "./frame_limiter.js";
import { computePlaybackRate } from "./audio_resampler.js";
import { planFrame } from "./frame_plan.js";
import { createSineScroller } from "./sine_scroller.js";
import { getKeyboardControllerTarget } from "./input_routing.js";
import { dispatchWebShortcutAction } from "./shortcut_actions.js";
import {
    buildFullHelpOverlayText,
    buildShortcutReferenceText,
    computeShortcutHelpFontSizePx,
    toggleShortcutHelpVisibility
} from "./shortcut_help.js";
import { createCrosshair } from "./crosshair.js";
import { computeFullscreenCanvasSize, computeWindowedCanvasSize } from "./canvas_size.js";
import {
    findNextVisibleZoomHeight,
} from "./zoom_controls.js";
import { createToastContainer, createToastOverlay, drainNesToasts } from "./toast_overlay.js";
import { createGamepadInitToastNotifier } from "./gamepad_init_toast.js";
import { renderDisasmLines } from "./debugger_disasm.js";
import { buildOamHtml } from "./debugger_oam.js";
import { formatWatchEntry, parseWatchAddressInput } from "./debugger_watch.js";
import {
    computeNtscDisplayWidth,
    computeScrollViewportRects,
} from "./ppu_viewer_layout.js";
import { clampScrollTop, sanitizeScrollTop } from "./ppu_viewer_scroll.js";
import { computeMouseCursorStyle } from "./cursor_visibility.js";
import {
    shouldForwardArkanoidMouseInput,
    shouldKeepPointerLocked,
} from "./pointer_lock.js";

const statusEl = document.getElementById("status");
const startBtn = document.getElementById("start");
const romInput = document.getElementById("rom");
const romSelect = document.getElementById("rom-select");
const canvas = document.getElementById("screen");
if (!(canvas instanceof HTMLCanvasElement)) {
    throw new Error("Canvas element with id 'screen' not found or not a canvas");
}
const screenWrap = canvas.closest(".screen-wrap");
if (!(screenWrap instanceof HTMLElement)) {
    throw new Error("Screen wrapper with class 'screen-wrap' not found");
}
const shortcutReference = document.getElementById("shortcut-reference");
const shortcutHelpOverlay = document.getElementById("shortcut-help-overlay");
const debuggerPanel = document.getElementById("debugger-panel");

// Use WebGL for rendering with filter support
const gl = canvas.getContext("webgl");
if (!gl) {
    throw new Error("WebGL rendering context not available for canvas 'screen'");
}

// NES display dimensions after overscan removal (updated after NES instance is created).
let width = 256 - 2 * 8; // default: horizontal_overscan=8 → 240
let height = 240 - 2 * 8; // default: vertical_overscan=8  → 224
const SCROLLER_TEXT = "Updates: Feb 24: Added recording/autorunner support and debugger support for web. Feb 19: Added scraping of ROM database for better comapatibility. Feb 14: Full PAL support! Keyboard shortcuts with 'H'. ** Feb 7: Added support for NES Zapper controller. ** Feb 5: Added support for Arkanoid controller!  **";
const SCROLLER_SPEED = 2.0;
const SCROLLER_AMPLITUDE = 20;
const SCROLLER_FREQUENCY = 0.05;
const SCROLLER_FONT_SIZE_PX = 20;
const SCROLLER_FONT_FAMILY = "'VT323', monospace";

const toastContainer = createToastContainer(screenWrap);

const toastOverlay = createToastOverlay({ container: toastContainer });

const gamepadInitToastNotifier = createGamepadInitToastNotifier({
    buildMessage: gamepad_init_toast_message,
    showToast: (message) => toastOverlay.show(message)
});

let wasmInitPromise = null;

function createWasmUrl() {
    const wasmUrl = new URL("./pkg/neser_bg.wasm", import.meta.url);
    wasmUrl.searchParams.set("v", "20260224-701");
    return wasmUrl;
}

function ensureWasmInitialized() {
    if (!wasmInitPromise) {
        wasmInitPromise = init({ module_or_path: createWasmUrl() });
    }
    return wasmInitPromise;
}

// WebGL shader setup for filters
const filters = {
    stock: {
        name: "None",
        type: "single",
        fragmentShader: `
            #ifdef GL_FRAGMENT_PRECISION_HIGH
                precision highp float;
            #else
                precision mediump float;
            #endif
            varying vec2 v_texCoord;
            uniform sampler2D u_texture;

            void main() {
                gl_FragColor = texture2D(u_texture, v_texCoord);
            }
        `
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
        fragmentShader: `
            #ifdef GL_FRAGMENT_PRECISION_HIGH
                precision highp float;
            #else
                precision mediump float;
            #endif
            varying vec2 v_texCoord;
            uniform sampler2D u_texture;
            uniform vec2 u_sourceSize;
            uniform vec2 u_outputSize;
            uniform float u_hardScan;
            uniform float u_hardPix;
            uniform float u_warpX;
            uniform float u_warpY;
            uniform float u_maskDark;
            uniform float u_maskLight;
            uniform float u_scaleInLinearGamma;
            uniform float u_shadowMask;
            uniform float u_brightBoost;
            uniform float u_hardBloomScan;
            uniform float u_hardBloomPix;
            uniform float u_bloomAmount;
            uniform float u_shape;

            #define DO_BLOOM 1

            float ToLinear1(float c) {
                if (u_scaleInLinearGamma == 0.0) {
                    return c;
                }
                return (c <= 0.04045) ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4);
            }

            vec3 ToLinear(vec3 c) {
                if (u_scaleInLinearGamma == 0.0) {
                    return c;
                }
                return vec3(ToLinear1(c.r), ToLinear1(c.g), ToLinear1(c.b));
            }

            float ToSrgb1(float c) {
                if (u_scaleInLinearGamma == 0.0) {
                    return c;
                }
                return (c < 0.0031308) ? c * 12.92 : 1.055 * pow(c, 0.41666) - 0.055;
            }

            vec3 ToSrgb(vec3 c) {
                if (u_scaleInLinearGamma == 0.0) {
                    return c;
                }
                return vec3(ToSrgb1(c.r), ToSrgb1(c.g), ToSrgb1(c.b));
            }

            vec3 Fetch(vec2 pos, vec2 off) {
                pos = (floor(pos * u_sourceSize + off) + vec2(0.5, 0.5)) / u_sourceSize;
                return ToLinear(u_brightBoost * texture2D(u_texture, pos.xy).rgb);
            }

            vec2 Dist(vec2 pos) {
                pos = pos * u_sourceSize;
                return -((pos - floor(pos)) - vec2(0.5));
            }

            float Gaus(float pos, float scale) {
                return exp2(scale * pow(abs(pos), u_shape));
            }

            vec3 Horz3(vec2 pos, float off) {
                vec3 b = Fetch(pos, vec2(-1.0, off));
                vec3 c = Fetch(pos, vec2(0.0, off));
                vec3 d = Fetch(pos, vec2(1.0, off));
                float dst = Dist(pos).x;

                float scale = u_hardPix;
                float wb = Gaus(dst - 1.0, scale);
                float wc = Gaus(dst + 0.0, scale);
                float wd = Gaus(dst + 1.0, scale);

                return (b * wb + c * wc + d * wd) / (wb + wc + wd);
            }

            vec3 Horz5(vec2 pos, float off) {
                vec3 a = Fetch(pos, vec2(-2.0, off));
                vec3 b = Fetch(pos, vec2(-1.0, off));
                vec3 c = Fetch(pos, vec2(0.0, off));
                vec3 d = Fetch(pos, vec2(1.0, off));
                vec3 e = Fetch(pos, vec2(2.0, off));

                float dst = Dist(pos).x;
                float scale = u_hardPix;
                float wa = Gaus(dst - 2.0, scale);
                float wb = Gaus(dst - 1.0, scale);
                float wc = Gaus(dst + 0.0, scale);
                float wd = Gaus(dst + 1.0, scale);
                float we = Gaus(dst + 2.0, scale);

                return (a * wa + b * wb + c * wc + d * wd + e * we) / (wa + wb + wc + wd + we);
            }

            vec3 Horz7(vec2 pos, float off) {
                vec3 a = Fetch(pos, vec2(-3.0, off));
                vec3 b = Fetch(pos, vec2(-2.0, off));
                vec3 c = Fetch(pos, vec2(-1.0, off));
                vec3 d = Fetch(pos, vec2(0.0, off));
                vec3 e = Fetch(pos, vec2(1.0, off));
                vec3 f = Fetch(pos, vec2(2.0, off));
                vec3 g = Fetch(pos, vec2(3.0, off));

                float dst = Dist(pos).x;
                float scale = u_hardBloomPix;
                float wa = Gaus(dst - 3.0, scale);
                float wb = Gaus(dst - 2.0, scale);
                float wc = Gaus(dst - 1.0, scale);
                float wd = Gaus(dst + 0.0, scale);
                float we = Gaus(dst + 1.0, scale);
                float wf = Gaus(dst + 2.0, scale);
                float wg = Gaus(dst + 3.0, scale);

                return (a * wa + b * wb + c * wc + d * wd + e * we + f * wf + g * wg) /
                    (wa + wb + wc + wd + we + wf + wg);
            }

            float Scan(vec2 pos, float off) {
                float dst = Dist(pos).y;
                return Gaus(dst + off, u_hardScan);
            }

            float BloomScan(vec2 pos, float off) {
                float dst = Dist(pos).y;
                return Gaus(dst + off, u_hardBloomScan);
            }

            vec3 Tri(vec2 pos) {
                vec3 a = Horz3(pos, -1.0);
                vec3 b = Horz5(pos, 0.0);
                vec3 c = Horz3(pos, 1.0);

                float wa = Scan(pos, -1.0);
                float wb = Scan(pos, 0.0);
                float wc = Scan(pos, 1.0);

                return a * wa + b * wb + c * wc;
            }

            vec3 Bloom(vec2 pos) {
                vec3 a = Horz5(pos, -2.0);
                vec3 b = Horz7(pos, -1.0);
                vec3 c = Horz7(pos, 0.0);
                vec3 d = Horz7(pos, 1.0);
                vec3 e = Horz5(pos, 2.0);

                float wa = BloomScan(pos, -2.0);
                float wb = BloomScan(pos, -1.0);
                float wc = BloomScan(pos, 0.0);
                float wd = BloomScan(pos, 1.0);
                float we = BloomScan(pos, 2.0);

                return a * wa + b * wb + c * wc + d * wd + e * we;
            }

            vec2 Warp(vec2 pos) {
                pos = pos * 2.0 - 1.0;
                pos *= vec2(1.0 + (pos.y * pos.y) * u_warpX, 1.0 + (pos.x * pos.x) * u_warpY);
                return pos * 0.5 + 0.5;
            }

            vec3 Mask(vec2 pos) {
                vec3 mask = vec3(u_maskDark);

                if (u_shadowMask == 1.0) {
                    float line = u_maskLight;
                    float odd = 0.0;

                    if (fract(pos.x * 0.166666666) < 0.5) odd = 1.0;
                    if (fract((pos.y + odd) * 0.5) < 0.5) line = u_maskDark;

                    pos.x = fract(pos.x * 0.333333333);

                    if (pos.x < 0.333) mask.r = u_maskLight;
                    else if (pos.x < 0.666) mask.g = u_maskLight;
                    else mask.b = u_maskLight;
                    mask *= line;
                } else if (u_shadowMask == 2.0) {
                    pos.x = fract(pos.x * 0.333333333);

                    if (pos.x < 0.333) mask.r = u_maskLight;
                    else if (pos.x < 0.666) mask.g = u_maskLight;
                    else mask.b = u_maskLight;
                } else if (u_shadowMask == 3.0) {
                    pos.x += pos.y * 3.0;
                    pos.x = fract(pos.x * 0.166666666);

                    if (pos.x < 0.333) mask.r = u_maskLight;
                    else if (pos.x < 0.666) mask.g = u_maskLight;
                    else mask.b = u_maskLight;
                } else if (u_shadowMask == 4.0) {
                    pos = floor(pos * vec2(1.0, 0.5));
                    pos.x += pos.y * 3.0;
                    pos.x = fract(pos.x * 0.166666666);

                    if (pos.x < 0.333) mask.r = u_maskLight;
                    else if (pos.x < 0.666) mask.g = u_maskLight;
                    else mask.b = u_maskLight;
                }

                return mask;
            }

            void main() {
                vec2 pos = Warp(v_texCoord);
                if (pos.x < 0.0 || pos.x > 1.0 || pos.y < 0.0 || pos.y > 1.0) {
                    gl_FragColor = vec4(0.0, 0.0, 0.0, 1.0);
                    return;
                }
                vec3 outColor = Tri(pos);

            #ifdef DO_BLOOM
                outColor.rgb += Bloom(pos) * u_bloomAmount;
            #endif

                if (u_shadowMask > 0.0) {
                    outColor.rgb *= Mask(v_texCoord * u_outputSize * 1.000001);
                }

                gl_FragColor = vec4(ToSrgb(outColor.rgb), 1.0);
            }
        `
    }
};

const vertexShaderSource = `
    attribute vec2 a_position;
    attribute vec2 a_texCoord;
    varying vec2 v_texCoord;
    varying vec2 v_pixelCoord;
    uniform vec2 u_textureSize;

    void main() {
        gl_Position = vec4(a_position, 0.0, 1.0);
        v_texCoord = a_texCoord;
        v_pixelCoord = a_texCoord * u_textureSize;
    }
`;

const ntscPass1VertexShaderSource = `
    attribute vec2 a_position;
    attribute vec2 a_texCoord;
    varying vec2 v_texCoord;
    varying vec2 v_pixNo;
    uniform vec2 u_outputSize;

    void main() {
        gl_Position = vec4(a_position, 0.0, 1.0);
        v_texCoord = a_texCoord;
        v_pixNo = a_texCoord * u_outputSize;
    }
`;

const ntscPass1FragmentShaderSource = `
    #ifdef GL_FRAGMENT_PRECISION_HIGH
      precision highp float;
    #else
      precision mediump float;
    #endif
    varying vec2 v_texCoord;
    varying vec2 v_pixNo;
    uniform sampler2D u_texture;
    uniform float u_frameCount;
    uniform float u_chromaEncode;

    #define PI 3.14159265
    #define CHROMA_MOD_FREQ (PI / 3.0)
    #define SATURATION 1.0
    #define BRIGHTNESS 1.0
    #define ARTIFACTING 1.0
    #define FRINGING 1.0

    const mat3 mix_mat = mat3(
      BRIGHTNESS, FRINGING, FRINGING,
      ARTIFACTING, 2.0 * SATURATION, 0.0,
      ARTIFACTING, 0.0, 2.0 * SATURATION
    );

    const mat3 yiq_mat = mat3(
      0.2989, 0.5870, 0.1140,
      0.5959, -0.2744, -0.3216,
      0.2115, -0.5229, 0.3114
    );

    vec3 rgb2yiq(vec3 col) {
        return col * yiq_mat;
    }

    void main() {
        vec3 col = texture2D(u_texture, v_texCoord).rgb;
        vec3 yiq = rgb2yiq(col);

        float chroma_phase = 0.6667 * PI * (mod(v_pixNo.y, 3.0) + u_frameCount);
        float mod_phase = chroma_phase + v_pixNo.x * CHROMA_MOD_FREQ;
        float i_mod = cos(mod_phase);
        float q_mod = sin(mod_phase);

        yiq.yz *= vec2(i_mod, q_mod); // Modulate.
        yiq *= mix_mat; // Cross-talk.
        yiq.yz *= vec2(i_mod, q_mod); // Demodulate.

        // Optional encoding for UNORM render targets: pack I/Q into 0..1
        yiq.yz = mix(yiq.yz, yiq.yz * 0.5 + 0.5, u_chromaEncode);

        gl_FragColor = vec4(yiq, 1.0);
    }
`;

const ntscPass2VertexShaderSource = `
    attribute vec2 a_position;
    attribute vec2 a_texCoord;
    varying vec2 v_texCoord;
    uniform vec2 u_sourceSize;

    void main() {
        gl_Position = vec4(a_position, 0.0, 1.0);
        vec2 flipped = vec2(a_texCoord.x, 1.0 - a_texCoord.y);
        v_texCoord = flipped - vec2(0.5 / u_sourceSize.x, 0.0);
    }
`;

const ntscPass2FragmentShaderSource = `
    #ifdef GL_FRAGMENT_PRECISION_HIGH
        precision highp float;
    #else
        precision mediump float;
    #endif
    varying vec2 v_texCoord;
    uniform sampler2D u_texture;
    uniform vec2 u_sourceSize;
    uniform float u_chromaEncode;
    uniform float u_chromaSum;

    #define TAPS 24
    #define NTSC_CRT_GAMMA 2.5
    #define NTSC_MONITOR_GAMMA 2.0

    float lumaTap(int i) {
        if (i == 0) return -0.000012020;
        if (i == 1) return -0.000022146;
        if (i == 2) return -0.000013155;
        if (i == 3) return -0.000012020;
        if (i == 4) return -0.000049979;
        if (i == 5) return -0.000113940;
        if (i == 6) return -0.000122150;
        if (i == 7) return -0.000005612;
        if (i == 8) return 0.000170516;
        if (i == 9) return 0.000237199;
        if (i == 10) return 0.000169640;
        if (i == 11) return 0.000285688;
        if (i == 12) return 0.000984574;
        if (i == 13) return 0.002018683;
        if (i == 14) return 0.002002275;
        if (i == 15) return -0.000909882;
        if (i == 16) return -0.007049081;
        if (i == 17) return -0.013222860;
        if (i == 18) return -0.012606931;
        if (i == 19) return 0.002460860;
        if (i == 20) return 0.035868225;
        if (i == 21) return 0.084016453;
        if (i == 22) return 0.135563500;
        if (i == 23) return 0.175261268;
        return 0.190176552;
    }

    float chromaTap(int i) {
        if (i == 0) return -0.000118847;
        if (i == 1) return -0.000271306;
        if (i == 2) return -0.000502642;
        if (i == 3) return -0.000930833;
        if (i == 4) return -0.001451013;
        if (i == 5) return -0.002064744;
        if (i == 6) return -0.002700432;
        if (i == 7) return -0.003241276;
        if (i == 8) return -0.003524948;
        if (i == 9) return -0.003350284;
        if (i == 10) return -0.002491729;
        if (i == 11) return -0.000721149;
        if (i == 12) return 0.002164659;
        if (i == 13) return 0.006313635;
        if (i == 14) return 0.011789103;
        if (i == 15) return 0.018545660;
        if (i == 16) return 0.026414396;
        if (i == 17) return 0.035100710;
        if (i == 18) return 0.044196567;
        if (i == 19) return 0.053207202;
        if (i == 20) return 0.061590275;
        if (i == 21) return 0.068803602;
        if (i == 22) return 0.074356193;
        if (i == 23) return 0.077856564;
        return 0.079052396;
    }

    const mat3 yiq2rgb_mat = mat3(
        1.0, 0.956, 0.6210,
        1.0, -0.2720, -0.6474,
        1.0, -1.1060, 1.7046
    );

    vec3 yiq2rgb(vec3 yiq) {
        return yiq * yiq2rgb_mat;
    }

    vec3 fetch_offset(float offset, float one_x) {
        return texture2D(u_texture, v_texCoord + vec2(offset * one_x, 0.0)).xyz;
    }

    void main() {
        float one_x = 1.0 / u_sourceSize.x;
        vec3 signal = vec3(0.0);
        for (int i = 0; i < TAPS; i++) {
            float offset = float(i);
            vec3 sums = fetch_offset(offset - float(TAPS), one_x) +
                fetch_offset(float(TAPS) - offset, one_x);
            float luma = lumaTap(i);
            float chroma = chromaTap(i);
            signal += sums * vec3(luma, chroma, chroma);
        }
        signal += texture2D(u_texture, v_texCoord).xyz *
            vec3(lumaTap(TAPS), chromaTap(TAPS), chromaTap(TAPS));

        // Optional decoding for UNORM render targets
        signal.yz = mix(signal.yz, signal.yz * 2.0 - vec2(u_chromaSum), u_chromaEncode);

        vec3 rgb = yiq2rgb(signal);
        gl_FragColor = vec4(pow(rgb, vec3(NTSC_CRT_GAMMA / NTSC_MONITOR_GAMMA)), 1.0);
    }
`;

let currentFilter = "ntsc"; // Start with NTSC filter as requested
const filterKeys = Object.keys(filters);
let shaderProgram = null;
let ntscPass1Program = null;
let ntscPass2Program = null;
let ntscPass1Texture = null;
let ntscPass1Framebuffer = null;
let ntscPass1TextureType = null;
let ntscPass1Width = width * 4;
let ntscPass1Height = height;
let ntscChromaEncode = 0.0;
const ntscChromaSum = 0.538021759;
let nesTexture = null;
let positionBuffer = null;
let texCoordBuffer = null;
let frameCount = 0; // For NTSC phase animation
const frameLimiter = createFrameLimiter(60);
const idleFrameLimiter = createFrameLimiter(60);
let webglInitialized = false; // Track WebGL initialization state
let idleScrollerActive = false;
let idleScroller = null;
let idleScrollerStartTime = 0;
let crosshair = null; // Crosshair overlay for Zapper
let windowFocused = true;
let pointerReleasedByEscape = false;
let lockedPointerX = 0;
let lockedPointerY = 0;

function requestPointerLockFromUserGesture() {
    pointerReleasedByEscape = false;
    if (document.pointerLockElement !== canvas) {
        canvas.requestPointerLock?.();
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
}

function cacheProgramLocations(program) {
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
}

function createProgram(vertexSource, fragmentSource) {
    const vertexShader = gl.createShader(gl.VERTEX_SHADER);
    gl.shaderSource(vertexShader, vertexSource);
    gl.compileShader(vertexShader);
    if (!gl.getShaderParameter(vertexShader, gl.COMPILE_STATUS)) {
        console.error("Vertex shader compilation failed:", gl.getShaderInfoLog(vertexShader));
        gl.deleteShader(vertexShader);
        return null;
    }

    const fragmentShader = gl.createShader(gl.FRAGMENT_SHADER);
    gl.shaderSource(fragmentShader, fragmentSource);
    gl.compileShader(fragmentShader);
    if (!gl.getShaderParameter(fragmentShader, gl.COMPILE_STATUS)) {
        console.error("Fragment shader compilation failed:", gl.getShaderInfoLog(fragmentShader));
        gl.deleteShader(fragmentShader);
        gl.deleteShader(vertexShader);
        return null;
    }

    const program = gl.createProgram();
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

    cacheProgramLocations(program);
    return program;
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

function setupFilterPrograms(filterName) {
    const filter = filters[filterName];
    if (!filter) {
        console.error("Unknown filter:", filterName);
        return false;
    }

    if (filter.type === "ntsc") {
        ntscPass1Program = createProgram(ntscPass1VertexShaderSource, ntscPass1FragmentShaderSource);
        ntscPass2Program = createProgram(ntscPass2VertexShaderSource, ntscPass2FragmentShaderSource);
        if (!ntscPass1Program || !ntscPass2Program) {
            return false;
        }
        if (!createNtscPass1Target()) {
            return false;
        }
        shaderProgram = null;
        return true;
    }

    shaderProgram = createProgram(vertexShaderSource, filter.fragmentShader);
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
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, width, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);

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
    const currentIndex = filterKeys.indexOf(currentFilter);
    const nextIndex = (currentIndex + 1) % filterKeys.length;
    const nextFilter = filterKeys[nextIndex];
    currentFilter = nextFilter;

    // Recreate buffers and textures but keep running state
    if (!initWebGL()) {
        console.error("Failed to switch filter");
        return false;
    }

    return true;
}

let nes;
let romBytes;
let romMetadata;
let saveStateController = null;
let saveStateAvailable = false;
let running = false;
let paused = false;

// ── Autorun context + DOM elements ───────────────────────────────────────────
const autorunCtx = createAutorunContext();
const autorunCreateCheckbox = document.getElementById("autorun-create");
const autorunLoadBtn = document.getElementById("autorun-load");
const autorunStatusEl = document.getElementById("autorun-status");
const autorunFileInput = document.getElementById("autorun-file-input");
const autorunFileInfo = document.getElementById("autorun-file-info");
const autorunFileSummary = document.getElementById("autorun-file-summary");
const autorunCheckpointSelect = document.getElementById("autorun-checkpoint-select");
const autorunExtendCheck = document.getElementById("autorun-extend-check");
const autorunUseBtn = document.getElementById("autorun-use-btn");
const autorunCancelBtn = document.getElementById("autorun-cancel");

/** Update the small autorun status text and cancel button in the header. */
function updateAutorunStatus() {
    if (!autorunStatusEl) return;
    const config = autorunCtx.getActiveConfig();
    if (!config) {
        autorunStatusEl.textContent = "";
        autorunCancelBtn?.classList.add("d-none");
    } else if (config.mode === "record") {
        autorunStatusEl.textContent = "Will record autorun";
        autorunCancelBtn?.classList.remove("d-none");
    } else {
        const info = autorunCtx.getLoadedFile();
        const cpText = config.checkpointIdx != null
            ? `from checkpoint ${config.checkpointIdx + 1}`
            : "from beginning";
        const extText = config.extend ? ", extending" : "";
        const expectedRom = autorunCtx.getExpectedRomName();
        const romHint = expectedRom ? ` · Load ${expectedRom}` : "";
        autorunStatusEl.textContent =
            `Autorun loaded (${info?.frameCount ?? "?"} frames, ${cpText}${extText})${romHint}`;
        autorunCancelBtn?.classList.remove("d-none");
    }
}

if (autorunCreateCheckbox) {
    autorunCreateCheckbox.addEventListener("change", () => {
        autorunCtx.setCreateRecording(autorunCreateCheckbox.checked);
        if (autorunCreateCheckbox.checked) {
            // Clear loaded file when switching to create-recording mode
            autorunCtx.clearLoadedFile();
        }
        updateAutorunStatus();
    });
}

if (autorunCancelBtn) {
    autorunCancelBtn.addEventListener("click", () => {
        autorunCtx.clearLoadedFile();
        autorunCtx.setCreateRecording(false);
        if (autorunCreateCheckbox) autorunCreateCheckbox.checked = false;
        updateAutorunStatus();
    });
}

// Open modal on "Load autorun" button click
if (autorunLoadBtn) {
    autorunLoadBtn.addEventListener("click", () => {
        // Reset modal state
        if (autorunFileInput) autorunFileInput.value = "";
        if (autorunFileInfo) autorunFileInfo.classList.add("d-none");
        if (autorunUseBtn) autorunUseBtn.disabled = true;
        if (autorunExtendCheck) autorunExtendCheck.checked = false;
        if (autorunCheckpointSelect) {
            while (autorunCheckpointSelect.options.length > 1) {
                autorunCheckpointSelect.remove(1);
            }
        }
        const modal = new window.bootstrap.Modal(document.getElementById("autorun-modal"));
        modal.show();
    });
}

// Handle autorun file selection inside modal
if (autorunFileInput) {
    autorunFileInput.addEventListener("change", async (e) => {
        const file = e.target.files?.[0];
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
            autorunFileInfo.classList.remove("d-none");
            autorunUseBtn.disabled = false;
            // Store raw bytes and filename on the input element for the Use button
            autorunFileInput._bytes = bytes;
            autorunFileInput._fileName = file.name;
        } catch (err) {
            autorunFileSummary.textContent = `Error: ${err.message}`;
            autorunFileInfo.classList.remove("d-none");
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
            const modalEl = document.getElementById("autorun-modal");
            const modal = window.bootstrap.Modal.getInstance(modalEl);
            modal?.hide();
        } catch (err) {
            console.error("Failed to configure autorun:", err);
        }
    });
}

/**
 * Update NES display dimensions from the WASM instance and reallocate the GL texture.
 * Must be called after `nes` is created so overscan is reflected.
 */
function updateNesDisplayDimensions() {
    if (!nes) return;
    width = nes.screen_width();
    height = nes.screen_height();
    NES_ASPECT_RATIO = width / height;
    // Reallocate the NES texture with the correct (cropped) dimensions.
    if (nesTexture) {
        gl.bindTexture(gl.TEXTURE_2D, nesTexture);
        gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, width, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
    }
}
let lastFrameTime = 0;
const fpsLogIntervalMs = 1000;
let fpsLastTime = 0;
let fpsFrames = 0;

// Web Audio API setup
let audioContext = null;
let nextAudioTime = 0;
const AUDIO_SAMPLE_RATE = 44100; // Target output sample rate for Web Audio (NES audio is downsampled to this rate)
const NES_APU_MAX = 1.177; // Conservative max output from NES APU mixer including expansion audio
const AUDIO_TARGET_LATENCY = 0.1; // seconds
const AUDIO_MAX_ADJUST = 0.005; // +/- 0.5% playback rate
const AUDIO_LATENCY_GAIN = 0.1; // scale factor for latency correction
let audioMuted = false;
let gamepadEnabled = true;
let lastGamepadState1 = {
    a: false,
    b: false,
    select: false,
    start: false,
    up: false,
    down: false,
    left: false,
    right: false
};
let lastGamepadState2 = {
    a: false,
    b: false,
    select: false,
    start: false,
    up: false,
    down: false,
    left: false,
    right: false
};

function setStatus(msg, isError = false) {
    statusEl.textContent = msg;
    statusEl.style.color = isError ? "#f88" : "#8fe28f";
}

async function applyRomBytes(bytes, name) {
    romBytes = bytes;
    romMetadata = {
        name,
        size: romBytes.length,
        bytes: romBytes
    };
    setStatus(`Loaded ROM: ${name} (${romBytes.length} bytes)`);
    stopIdleScroller();
    await refreshSaveStateController();
}

async function refreshSaveStateController() {
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
            setStatus
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

romInput.addEventListener("change", async (e) => {
    const file = e.target.files?.[0];
    if (!file) return;
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
        const value = e.target.value;
        if (!value) return;
        requestPointerLockFromUserGesture();
        try {
            const response = await fetch(value);
            if (!response.ok) {
                throw new Error(`HTTP ${response.status}`);
            }
            const bytes = new Uint8Array(await response.arrayBuffer());
            const name = value.split("/").pop() || value;
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

function initAudioContext() {
    if (!audioContext) {
        // Create AudioContext on first user interaction (required by browsers)
        audioContext = new (window.AudioContext || window.webkitAudioContext)({
            sampleRate: AUDIO_SAMPLE_RATE
        });
        nextAudioTime = audioContext.currentTime;
        console.log(`Audio initialized: ${audioContext.sampleRate} Hz`);
    }
}

function playAudioSamples(samples) {
    if (!audioContext || audioMuted || samples.length === 0) return;

    // Create an audio buffer for the samples
    const buffer = audioContext.createBuffer(1, samples.length, audioContext.sampleRate);
    const channelData = buffer.getChannelData(0);

    // Normalize and copy samples to the buffer
    // NES APU outputs 0.0 to ~1.177, normalize to 0.0 to 1.0 for Web Audio
    for (let i = 0; i < samples.length; i++) {
        // Map NES 0.0-1.177 to Web Audio 0.0-1.0 (0.0 represents silence)
        const normalized = samples[i] / NES_APU_MAX;
        channelData[i] = Math.min(1.0, Math.max(0.0, normalized));
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
    if (startBtn.disabled) {
        return;
    }
    startBtn.disabled = true;
    if (!romBytes) {
        setStatus("Please choose a ROM first", true);
        startBtn.disabled = false;
        return;
    }
    stopIdleScroller();
    setStatus("Initializing emulator...");
    try {
        if (!nes) {
            await ensureWasmInitialized();

            // Initialize WebGL shaders before creating NES instance
            if (!initWebGL()) {
                throw new Error("Failed to initialize WebGL");
            }

            nes = new WasmNes();
            updateNesDisplayDimensions();
        }
        const romName = romMetadata?.name || "selected-rom.nes";

        // ── Autorun setup: configure before loading ROM ─────────────────────
        const autorunConfig = autorunCtx.getActiveConfig();
        if (autorunConfig?.mode === "record") {
            nes.start_autorun_recording();
        } else {
            nes.clear_autorun();
        }

        nes.load_rom(romBytes, romName);
        drainNesToasts(nes, toastOverlay);

        // ── Autorun setup: playback / extend – after ROM is loaded ───────────
        if (autorunConfig?.mode === "playback") {
            try {
                const pendingRestore = nes.load_autorun_playback(
                    autorunConfig.bytes,
                    autorunConfig.checkpointIdx ?? -1,
                    autorunConfig.extend
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

        frameLimiter.setTargetFps(nes.frame_rate_hz());
        // Initialize audio context on user interaction (browser requirement)
        initAudioContext();
        nes.set_audio_muted(audioMuted);
        await refreshSaveStateController();
        
        // Update pointer visibility and Zapper overlay after ROM is loaded
        updateMouseCursorState();
    } catch (err) {
        drainNesToasts(nes, toastOverlay);
        setStatus(`Failed to load ROM: ${err}`, true);
        startBtn.disabled = false;
        // Only reset nes if wasm/webgl initialization failed
        // Don't reset on simple ROM load errors so we can retry
        if (err.message && err.message.includes("WebGL")) {
            nes = null;
            webglInitialized = false;
        }
        return;
    }
    running = true;
    paused = false;
    // Keep start button disabled while running; it is re-enabled in stop().
    setStatus("Running...");
    requestAnimationFrame(step);
}

function resumeFrameLoop() {
    lastFrameTime = 0;
    frameLimiter.reset();
    setStatus("Running...");
    requestAnimationFrame(step);
}

function pauseResume() {
    if (!nes || !running) return;
    paused = !paused;
    if (!paused) {
        resumeFrameLoop();
    } else {
        setStatus("Paused");
    }
}

let debuggerHexdumpError = "";
let debuggerWatchAddError = "";
let debuggerWatchRowErrors = new Map();

function buildDisasmHtml(nes) {
    try {
        const disasmJson = nes.debugger_disasm_json();
        const lines = JSON.parse(disasmJson);
        return renderDisasmLines(lines);
    } catch (_) { /* disasm not available yet */ }
    return "";
}

function formatStatusFlags(p) {
    const flag = (bit, ch) => (p & (1 << bit)) ? ch : "-";
    return flag(7,"N") + flag(6,"V") + flag(5,"U") + flag(4,"B") +
           flag(3,"D") + flag(2,"I") + flag(1,"Z") + flag(0,"C");
}

function buildRegsHtml(snap) {
    const h2 = n => n.toString(16).toUpperCase().padStart(2, "0");
    const h4 = n => n.toString(16).toUpperCase().padStart(4, "0");
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
    return lines.map(l => {
        const esc = l.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
        return `<span>${esc}</span>`;
    }).join("\n");
}

function formatHexdumpLines(baseAddr, bytes) {
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
            .map((value) => (value >= 0x20 && value <= 0x7E ? String.fromCharCode(value) : "."))
            .join("");
        lines.push(`${addr.toString(16).toUpperCase().padStart(4, "0")}: ${hexParts.join(" ")} |${ascii}|`);
    }
    return lines;
}

function buildHexdumpHtml(snap) {
    const base = Number.isInteger(snap.prg_hexdump_base) ? snap.prg_hexdump_base : 0;
    const lines = formatHexdumpLines(base, snap.prg_hexdump_bytes);
    const escLine = (line) => line.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
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

function buildWatchHtml(snap) {
    const watchValues = Array.isArray(snap.watch_values) ? snap.watch_values : [];
    const esc = (value) => String(value)
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;");

    const rows = watchValues.map((entry, index) => {
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

function buildTraceHtml(snap) {
    const traceLines = Array.isArray(snap.recent_trace) ? snap.recent_trace : [];
    const esc = (value) => String(value)
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;");

    const rows = traceLines.map((entry) => {
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

function drawRgbaToCanvas(canvasId, rgbaBytes, width, height, displayWidth = width) {
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

    const scrollJson = nes.debugger_ppu_scroll_json();
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

function buildPpuViewerHtml(isVisible) {
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
        `<button class="dbg-btn" id="dbg-toggle-ppu-viewer">${ppuViewerButtonText}</button>` +
        `<button class="dbg-btn" id="dbg-step-over">Step over (F10)</button>` +
        `<button class="dbg-btn" id="dbg-step-into">Step into (F11)</button>` +
        `<button class="dbg-btn" id="dbg-continue">Continue (F5)</button>` +
        `<button class="dbg-btn" id="dbg-run-next-frame">Run to next frame</button>` +
        `<button class="dbg-btn" id="dbg-run-next-scanline">Run to next scanline</button>` +
        `<button class="dbg-btn" id="dbg-run-to-nmi">Run to NMI</button>` +
        `<button class="dbg-btn" id="dbg-run-to-irq">Run to IRQ</button>` +
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
    function wire(id, handler) {
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
        el.addEventListener("keydown", (e) => {
            if (e.key !== "Enter") {
                return;
            }
            e.preventDefault();
            const index = Number(el.id.replace("dbg-watch-addr-", ""));
            if (!Number.isInteger(index)) {
                return;
            }
            debuggerWatchUpdateAddress(index, el.value);
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

function debuggerWatchRemoveAddress(index) {
    if (!nes) return;
    debuggerWatchRowErrors.delete(index);
    nes.debugger_watch_remove(index);
    updateDebuggerPanel();
}

function debuggerWatchUpdateAddress(index, value) {
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
    debuggerPanel.classList.remove("d-none");
    updateDebuggerPanel();
    setStatus("Debugger paused");
}

function hideDebuggerPanel() {
    if (debuggerPanel) {
        debuggerPanel.classList.add("d-none");
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

function parseHexdumpAddressInput(rawInput) {
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
        debuggerHexdumpError = parsed.error;
        showDebuggerPanel();
        return;
    }

    debuggerHexdumpError = "";
    nes.debugger_hexdump_set_base(parsed.value);
    showDebuggerPanel();
}

function stop() {
    // ── Autorun teardown: download recording if active ────────────────────
    if (nes && nes.autorun_is_recording()) {
        const recordingBytes = nes.stop_autorun();
        if (recordingBytes && recordingBytes.length > 0) {
            triggerAutorunDownload(recordingBytes, romMetadata?.name);
        }
    } else if (nes) {
        nes.clear_autorun();
    }

    running = false;
    paused = false;
    startBtn.disabled = false;
    clearCanvas();
    lastFrameTime = 0;
    frameLimiter.reset();
    if (document.pointerLockElement === canvas) {
        document.exitPointerLock?.();
    }
    document.body.style.cursor = "default";
    windowFocused = true;
    pointerReleasedByEscape = false;
    setStatus("Stopped. You can restart or load a new ROM");
}

/**
 * Trigger a browser download of an autorun recording.
 * @param {Uint8Array} bytes - The serialized AutorunFile JSON bytes.
 * @param {string|undefined} romName - ROM file name (used to derive the download file name).
 */
function triggerAutorunDownload(bytes, romName) {
    const baseName = romName ? romName.replace(/\.nes$/i, "") : "recording";
    const fileName = `${baseName}.autorun`;
    const blob = new Blob([bytes], { type: "application/json" });
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

function stepIdleScroller(timestamp) {
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

    const frame = idleScroller.renderFrame(timestamp);
    const filter = filters[currentFilter];
    let rendered = false;
    if (filter?.type === "ntsc") {
        rendered = renderNtscPass(frame);
    } else {
        rendered = renderSinglePass(frame);
    }

    if (!rendered) {
        idleScrollerActive = false;
        setStatus("Rendering error occurred. Please restart.", true);
        return;
    }

    frameCount = (frameCount + 1) % 3600;
    requestAnimationFrame(stepIdleScroller);
}

function bindQuadAttributes(program) {
    if (program._aPositionLocation !== -1 && program._aPositionLocation !== null) {
        gl.bindBuffer(gl.ARRAY_BUFFER, positionBuffer);
        gl.enableVertexAttribArray(program._aPositionLocation);
        gl.vertexAttribPointer(program._aPositionLocation, 2, gl.FLOAT, false, 0, 0);
    }
    if (program._aTexCoordLocation !== -1 && program._aTexCoordLocation !== null) {
        gl.bindBuffer(gl.ARRAY_BUFFER, texCoordBuffer);
        gl.enableVertexAttribArray(program._aTexCoordLocation);
        gl.vertexAttribPointer(program._aTexCoordLocation, 2, gl.FLOAT, false, 0, 0);
    }
}

function renderSinglePass(frame) {
    if (!shaderProgram) {
        console.error("Shader program is null, cannot render");
        return false;
    }

    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, nesTexture);
    gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, width, height, gl.RGBA, gl.UNSIGNED_BYTE, frame);

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

function renderNtscPass(frame) {
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

function step(timestamp) {
    if (!running || paused) return;
    lastFrameTime = timestamp;
    const { shouldStep, shouldRender } = planFrame({
        shouldRender: frameLimiter.shouldRender(timestamp)
    });
    try {
        if (gamepadEnabled && nes) {
            pollGamepad();
        }

        if (!shouldStep) {
            requestAnimationFrame(step);
            return;
        }

        const frame = nes.render_frame_rgba(); // RGBA8888

        // Stop when pure autorun playback has consumed all recorded frames
        if (nes.autorun_playback_finished()) {
            stop();
            setStatus("Autorun playback complete.");
            return;
        }

        const filter = filters[currentFilter];
        let rendered = true;
        if (shouldRender) {
            if (filter?.type === "ntsc") {
                rendered = renderNtscPass(frame);
            } else {
                rendered = renderSinglePass(frame);
            }
        }

        if (!rendered) {
            running = false;
            setStatus("Rendering error occurred. Please restart.", true);
            return;
        }

        // Increment frame counter for NTSC phase animation
        // Wrap at 3600 to prevent float precision issues (60 frames/sec * 60 sec = 3600 frames/min)
        frameCount = (frameCount + 1) % 3600;

        // Get and play audio samples
        const audioSamples = nes.get_audio_samples();
        if (audioSamples.length > 0) {
            playAudioSamples(audioSamples);
        }

        fpsFrames += 1;
        if (fpsLastTime === 0) {
            fpsLastTime = timestamp;
        }
        const fpsElapsed = timestamp - fpsLastTime;
        if (fpsElapsed >= fpsLogIntervalMs) {
            const fps = (fpsFrames * 1000) / fpsElapsed;
            console.log(`FPS: ${fps.toFixed(1)}`);
            fpsFrames = 0;
            fpsLastTime = timestamp;
        }
    } catch (err) {
        running = false;
        startBtn.disabled = false;
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
const gamepadToggleBtn = document.getElementById("gamepad-toggle");
function updateGamepadButton() {
    gamepadToggleBtn.textContent = gamepadEnabled ? "Gamepad : On" : "Gamepad : Off";
    gamepadToggleBtn.setAttribute("aria-pressed", gamepadEnabled ? "true" : "false");
}
gamepadToggleBtn.addEventListener("click", () => {
    gamepadEnabled = !gamepadEnabled;
    updateGamepadButton();
    if (!gamepadEnabled) {
        resetGamepadState();
    }
});
updateGamepadButton();
const muteBtn = document.getElementById("mute");
function updateMuteButton() {
    muteBtn.textContent = audioMuted ? "Audio: Off" : "Audio: On";
    muteBtn.setAttribute("aria-pressed", audioMuted ? "true" : "false");
}
muteBtn.addEventListener("click", async () => {
    audioMuted = !audioMuted;
    updateMuteButton();
    if (nes) {
        nes.set_audio_muted(audioMuted);
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
const pauseBtn = document.getElementById("pause");
const stopBtn = document.getElementById("stop");
const resetBtn = document.getElementById("reset");
if (!pauseBtn || !stopBtn || !resetBtn) {
    throw new Error("Pause/Stop/Reset buttons not found in DOM");
}
pauseBtn.addEventListener("click", pauseResume);
stopBtn.addEventListener("click", stop);
resetBtn.addEventListener("click", () => {
    resetAction();
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

// Keyboard input mappings for both controllers
// Controller 1: W=Up, S=Down, A=Left, D=Right, T=B, R=A, 4=Select, 5=Start
const keyToButtonController1 = {
    'w': { button: 4, name: 'Up' },      // Button 4 = Up
    's': { button: 5, name: 'Down' },    // Button 5 = Down
    'a': { button: 6, name: 'Left' },    // Button 6 = Left
    'd': { button: 7, name: 'Right' },   // Button 7 = Right
    't': { button: 1, name: 'B' },       // Button 1 = B
    'r': { button: 0, name: 'A' },       // Button 0 = A
    '4': { button: 2, name: 'Select' },  // Button 2 = Select
    '5': { button: 3, name: 'Start' }    // Button 3 = Start
};

// Controller 2: I=Up, K=Down, J=Left, L=Right, P=B, O=A, 9=Select, 0=Start
const keyToButtonController2 = {
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
let connectedGamepads = [];

function updateConnectedGamepads() {
    const gamepads = navigator.getGamepads ? navigator.getGamepads() : [];
    connectedGamepads = selectGamepads(gamepads);
    return connectedGamepads;
}

function showPageLoadGamepadInitToast() {
    if (gamepadEnabled) {
        toastOverlay.show("Press a button on any connected gamepad");
    }
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
};

function updateShortcutHelpOverlayText() {
    if (shortcutHelpOverlay) {
        shortcutHelpOverlay.textContent = buildFullHelpOverlayText(connectedGamepads.length);
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

function applyKeyboardMapping(event, mapping, controller, targets, pressed) {
    if (!mapping || !targets.includes(controller)) {
        return;
    }
    event.preventDefault();
    applyJoypadButtonIfAllowed(nes, controller, mapping.button, pressed);
}

function isEditableKeyboardTarget(target) {
    if (!(target instanceof HTMLElement)) {
        return false;
    }
    const tag = target.tagName.toUpperCase();
    return target.isContentEditable || tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
}

async function handleKeyDown(event) {
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

    if (!nes && event.code !== "KeyH") {
        return;
    }

    const handledShortcut = await dispatchWebShortcutAction(event, webShortcutActions);
    if (handledShortcut) {
        return;
    }

    if (!nes) {
        return;
    }

    if (nes.is_debugger_open()) {
        return;
    }

    const key = event.key.toLowerCase();
    const targets = getKeyboardControllerTarget(connectedGamepads.length);

    applyKeyboardMapping(event, keyToButtonController1[key], 1, targets, true);
    applyKeyboardMapping(event, keyToButtonController2[key], 2, targets, true);
}

function handleKeyUp(event) {
    if (isEditableKeyboardTarget(event.target)) {
        return;
    }

    if (!nes) {
        return;
    }

    if (nes.is_debugger_open()) {
        return;
    }

    const key = event.key.toLowerCase();
    const targets = getKeyboardControllerTarget(connectedGamepads.length);

    applyKeyboardMapping(event, keyToButtonController1[key], 1, targets, false);
    applyKeyboardMapping(event, keyToButtonController2[key], 2, targets, false);
}

document.addEventListener('keydown', handleKeyDown);
document.addEventListener('keyup', handleKeyUp);

function handleMouseMotion(event) {
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

function isArkanoidControllerActive(nesInstance) {
    if (!nesInstance) {
        return false;
    }

    const mouseOnAnyPort =
        nesInstance.is_mouse_emulated_controller(1) ||
        nesInstance.is_mouse_emulated_controller(2);
    return mouseOnAnyPort && !isZapperActive(nesInstance);
}

function isMouseControllerActive(nesInstance) {
    if (!nesInstance) {
        return false;
    }

    return (
        nesInstance.is_mouse_emulated_controller(1) ||
        nesInstance.is_mouse_emulated_controller(2)
    );
}

function setCrosshairVisible(visible) {
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
    if (!nes) return;

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

function handleMouseButton(event, pressed) {
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
const screenMinusBtn = document.getElementById("screen-minus");
const screenPlusBtn = document.getElementById("screen-plus");
const fullscreenBtn = document.getElementById("fullscreen");
const filterToggleBtn = document.getElementById("filter-toggle");
const saveStateBtn = document.getElementById("save-state");
const loadStateBtn = document.getElementById("load-state");

// NES native resolution is 256x240 pixels; aspect ratio updated after NES init.
let NES_ASPECT_RATIO = width / height;
const SCALE_STEP = 120; // Change height by 120px each step
const INITIAL_HEIGHT = 720; // Initial display height in pixels
let currentHeight = INITIAL_HEIGHT;

function applyCanvasSize(size) {
    canvas.style.width = size.cssWidth;
    canvas.style.height = size.cssHeight;
    canvas.width = size.pixelWidth;
    canvas.height = size.pixelHeight;
}

function updateCanvasSize(newHeight) {
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

function updateShortcutHelpScale() {
    if (!shortcutHelpOverlay) {
        return;
    }
    const fontSizePx = computeShortcutHelpFontSizePx(canvas.clientHeight);
    shortcutHelpOverlay.style.fontSize = `${fontSizePx}px`;
}

function measureDisplayHeightAt(height) {
    updateCanvasSize(height);
    return canvas.clientHeight;
}

function probeNextVisibleZoomHeight(direction) {
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

function updateZoomButtonState() {
    const inScreenFullscreen = document.fullscreenElement === screenWrap;
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

function applyZoom(direction) {
    if (document.fullscreenElement === screenWrap) {
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
    const inScreenFullscreen = document.fullscreenElement === screenWrap;
    fullscreenBtn.textContent = inScreenFullscreen ? "Exit Fullscreen" : "Fullscreen";
}

async function toggleScreenFullscreen() {
    if (document.fullscreenElement !== screenWrap) {
        try {
            await screenWrap.requestFullscreen();
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

function resetAction() {
    if (!nes) return;
    nes.reset(true);
    setStatus("Soft reset", false);
}

function hardResetAction() {
    if (!nes) return;
    nes.reset(false);
    setStatus("Hard reset", false);
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
    const enabled = Boolean(saveStateController);
    if (saveStateBtn) saveStateBtn.disabled = !enabled;
    if (loadStateBtn) loadStateBtn.disabled = !enabled || !saveStateAvailable;
}

// Set initial canvas size and button text
updateCanvasSize(INITIAL_HEIGHT);
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

function applyGamepadState(state, controller, lastState) {
    if (!nes) return;
    if (state.a !== lastState.a) {
        applyJoypadButtonIfAllowed(nes, controller, 0, state.a);
    }
    if (state.b !== lastState.b) {
        applyJoypadButtonIfAllowed(nes, controller, 1, state.b);
    }
    if (state.select !== lastState.select) {
        applyJoypadButtonIfAllowed(nes, controller, 2, state.select);
    }
    if (state.start !== lastState.start) {
        applyJoypadButtonIfAllowed(nes, controller, 3, state.start);
    }
    if (state.up !== lastState.up) {
        applyJoypadButtonIfAllowed(nes, controller, 4, state.up);
    }
    if (state.down !== lastState.down) {
        applyJoypadButtonIfAllowed(nes, controller, 5, state.down);
    }
    if (state.left !== lastState.left) {
        applyJoypadButtonIfAllowed(nes, controller, 6, state.left);
    }
    if (state.right !== lastState.right) {
        applyJoypadButtonIfAllowed(nes, controller, 7, state.right);
    }
}

function resetGamepadState() {
    const emptyState = {
        a: false,
        b: false,
        select: false,
        start: false,
        up: false,
        down: false,
        left: false,
        right: false
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
        .then(() => toastOverlay.show(gamepad_init_toast_message(gamepadEnabled, connectedGamepads.length)))
        .catch(() => {});
}

window.addEventListener("gamepadconnected", () => {
    onGamepadConnectionChanged();
    if (gamepadEnabled && running && !paused) {
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
    if (document.fullscreenElement === screenWrap) {
        updateCanvasSizeForFullscreenViewport();
    } else {
        // Exited fullscreen - restore previous size
        updateCanvasSize(currentHeight);
    }
    updateZoomButtonState();
});

// Re-fit canvas when viewport resizes while in fullscreen (e.g. orientation change)
window.addEventListener("resize", () => {
    if (document.fullscreenElement === screenWrap) {
        updateCanvasSizeForFullscreenViewport();
        updateZoomButtonState();
        return;
    }

    updateZoomButtonState();
});
