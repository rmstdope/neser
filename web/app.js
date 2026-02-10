import init, { WasmNes } from "./pkg/neser.js?v=20260127";
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
import { createFrameLimiter } from "./frame_limiter.js";
import { computePlaybackRate } from "./audio_resampler.js";
import { planFrame } from "./frame_plan.js";
import { createSineScroller } from "./sine_scroller.js";
import { getKeyboardControllerTarget } from "./input_routing.js";
import { createCrosshair } from "./crosshair.js";

const statusEl = document.getElementById("status");
const startBtn = document.getElementById("start");
const romInput = document.getElementById("rom");
const romSelect = document.getElementById("rom-select");
const canvas = document.getElementById("screen");
if (!(canvas instanceof HTMLCanvasElement)) {
    throw new Error("Canvas element with id 'screen' not found or not a canvas");
}

// Use WebGL for rendering with filter support
const gl = canvas.getContext("webgl");
if (!gl) {
    throw new Error("WebGL rendering context not available for canvas 'screen'");
}

const width = 256;
const height = 240;
const SCROLLER_TEXT = "Newest update: ** Feb 7: Added support for NES Zapper controller. Duck Hunt is now playable! Feb 5: Added support for Arkanoid controller - Use the mouse! **";
const SCROLLER_SPEED = 2.0;
const SCROLLER_AMPLITUDE = 40;
const SCROLLER_FREQUENCY = 0.009;
const SCROLLER_FONT_SIZE_PX = 20;
const SCROLLER_FONT_FAMILY = "'VT323', monospace";

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
    await handleRomSelection({
        bytes: new Uint8Array(await file.arrayBuffer()),
        name: file.name,
        running,
        stop,
        applyRomBytes,
        start
    });
});

if (romSelect) {
    romSelect.addEventListener("change", async (e) => {
        const value = e.target.value;
        if (!value) return;
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
                start
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
            const wasmUrl = new URL("./pkg/neser_bg.wasm", import.meta.url);
            wasmUrl.searchParams.set("v", "20260127");
            await init({ module_or_path: wasmUrl });

            // Initialize WebGL shaders before creating NES instance
            if (!initWebGL()) {
                throw new Error("Failed to initialize WebGL");
            }

            nes = new WasmNes();
        }
        nes.load_rom(romBytes);
        // Initialize audio context on user interaction (browser requirement)
        initAudioContext();
        nes.set_audio_muted(audioMuted);
        await refreshSaveStateController();
        
        // Update Zapper cursor state after ROM is loaded
        updateZapperCursor();
    } catch (err) {
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

function pauseResume() {
    if (!nes || !running) return;
    paused = !paused;
    if (!paused) {
        lastFrameTime = 0;
        frameLimiter.reset();
        setStatus("Running...");
        requestAnimationFrame(step);
    } else {
        setStatus("Paused");
    }
}

function stop() {
    running = false;
    paused = false;
    startBtn.disabled = false;
    clearCanvas();
    lastFrameTime = 0;
    frameLimiter.reset();
    setStatus("Stopped. You can restart or load a new ROM");
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
    const framePlan = planFrame({
        shouldRender: frameLimiter.shouldRender(timestamp)
    });
    try {
        if (gamepadEnabled && nes) {
            pollGamepad();
        }
        const frame = nes.render_frame_rgba(); // RGBA8888
        const filter = filters[currentFilter];
        let rendered = true;
        if (framePlan.shouldRender) {
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

startBtn.addEventListener("click", start);
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
    if (!nes) return;
    nes.reset();
    setStatus("Reset", false);
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
// Controller 1: W=Up, S=Down, A=Left, D=Right, G=B, F=A, R=Select, T=Start
const keyToButtonController1 = {
    'w': { button: 4, name: 'Up' },      // Button 4 = Up
    's': { button: 5, name: 'Down' },    // Button 5 = Down
    'a': { button: 6, name: 'Left' },    // Button 6 = Left
    'd': { button: 7, name: 'Right' },   // Button 7 = Right
    'g': { button: 1, name: 'B' },       // Button 1 = B
    'f': { button: 0, name: 'A' },       // Button 0 = A
    'r': { button: 2, name: 'Select' },  // Button 2 = Select
    't': { button: 3, name: 'Start' }    // Button 3 = Start
};

// Controller 2: U=Up, J=Down, H=Left, K=Right, ;=B, L=A, O=Select, P=Start
const keyToButtonController2 = {
    'u': { button: 4, name: 'Up' },      // Button 4 = Up
    'j': { button: 5, name: 'Down' },    // Button 5 = Down
    'h': { button: 6, name: 'Left' },    // Button 6 = Left
    'k': { button: 7, name: 'Right' },   // Button 7 = Right
    ';': { button: 1, name: 'B' },       // Button 1 = B
    'l': { button: 0, name: 'A' },       // Button 0 = A
    'o': { button: 2, name: 'Select' },  // Button 2 = Select
    'p': { button: 3, name: 'Start' }    // Button 3 = Start
};

// Track connected gamepads for routing
let connectedGamepads = [];

function updateConnectedGamepads() {
    const gamepads = navigator.getGamepads ? navigator.getGamepads() : [];
    connectedGamepads = selectGamepads(gamepads);
    return connectedGamepads;
}

// Initialize connectedGamepads to detect any gamepads already connected on page load
updateConnectedGamepads();

document.addEventListener('keydown', (e) => {
    if (!nes) return;
    const key = e.key.toLowerCase();
    const targets = getKeyboardControllerTarget(connectedGamepads.length);
    
    // Check controller 1 mapping
    const mapping1 = keyToButtonController1[key];
    if (mapping1 && targets.includes(1)) {
        e.preventDefault();
        applyJoypadButtonIfAllowed(nes, 1, mapping1.button, true);
    }
    
    // Check controller 2 mapping
    const mapping2 = keyToButtonController2[key];
    if (mapping2 && targets.includes(2)) {
        e.preventDefault();
        applyJoypadButtonIfAllowed(nes, 2, mapping2.button, true);
    }
});

document.addEventListener('keyup', (e) => {
    if (!nes) return;
    const key = e.key.toLowerCase();
    const targets = getKeyboardControllerTarget(connectedGamepads.length);
    
    // Check controller 1 mapping
    const mapping1 = keyToButtonController1[key];
    if (mapping1 && targets.includes(1)) {
        e.preventDefault();
        applyJoypadButtonIfAllowed(nes, 1, mapping1.button, false);
    }
    
    // Check controller 2 mapping
    const mapping2 = keyToButtonController2[key];
    if (mapping2 && targets.includes(2)) {
        e.preventDefault();
        applyJoypadButtonIfAllowed(nes, 2, mapping2.button, false);
    }
});

function handleMouseMotion(event) {
    if (!nes) return;
    const rect = canvas.getBoundingClientRect();
    if (rect.width <= 1 || rect.height <= 1) {
        return;
    }
    const x = event.clientX - rect.left;
    const y = event.clientY - rect.top;
    applyMouseMotion(nes, x, y, rect.width, rect.height);
    
    // Update crosshair position if visible
    if (crosshair && crosshair.visible) {
        crosshair.updatePosition(x, y);
    }
}

function updateZapperCursor() {
    if (!nes) return;
    
    const zapperActive = isZapperActive(nes);
    
    if (zapperActive) {
        // Hide system cursor and show crosshair
        canvas.style.cursor = "none";
        if (!crosshair) {
            crosshair = createCrosshair(canvas);
        }
        crosshair.show();
    } else {
        // Show system cursor and hide/destroy crosshair
        canvas.style.cursor = "default";
        if (crosshair) {
            crosshair.destroy();
            crosshair = null;
        }
    }
}

function handleMouseButton(event, pressed) {
    if (!nes) return;
    applyMouseButton(nes, event.button, pressed);
}

canvas.addEventListener("mousemove", handleMouseMotion);
canvas.addEventListener("mousedown", (event) => handleMouseButton(event, true));
window.addEventListener("mouseup", (event) => handleMouseButton(event, false));

// Screen size controls
const screenMinusBtn = document.getElementById("screen-minus");
const screenPlusBtn = document.getElementById("screen-plus");
const fullscreenBtn = document.getElementById("fullscreen");
const filterToggleBtn = document.getElementById("filter-toggle");
const saveStateBtn = document.getElementById("save-state");
const loadStateBtn = document.getElementById("load-state");

// NES native resolution is 256x240 pixels
const NES_ASPECT_RATIO = 256 / 240;
const SCALE_STEP = 120; // Change height by 120px each step
const INITIAL_HEIGHT = 720; // Initial display height in pixels
let currentHeight = INITIAL_HEIGHT;

function updateCanvasSize(newHeight) {
    currentHeight = Math.max(240, Math.min(newHeight, 1440)); // Clamp between 240 and 1440
    const newWidth = Math.round(currentHeight * NES_ASPECT_RATIO);
    canvas.style.width = `${newWidth}px`;
    canvas.style.height = `${currentHeight}px`;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(newWidth * dpr);
    canvas.height = Math.round(currentHeight * dpr);
    
    // Update crosshair overlay size if it exists
    if (crosshair) {
        crosshair.updateCanvasSize();
    }
}

// Update fullscreen button text based on state
function updateFullscreenButton() {
    fullscreenBtn.textContent = document.fullscreenElement ? "Exit Fullscreen" : "Fullscreen";
}

function updateSaveStateButtons() {
    const enabled = Boolean(saveStateController);
    if (saveStateBtn) saveStateBtn.disabled = !enabled;
    if (loadStateBtn) loadStateBtn.disabled = !enabled || !saveStateAvailable;
}

// Set initial canvas size and button text
updateCanvasSize(INITIAL_HEIGHT);
updateFullscreenButton();
filterToggleBtn.textContent = `Filter: ${filters[currentFilter].name}`;
updateSaveStateButtons();
startIdleScroller();

screenMinusBtn.addEventListener("click", () => {
    updateCanvasSize(currentHeight - SCALE_STEP);
});

screenPlusBtn.addEventListener("click", () => {
    updateCanvasSize(currentHeight + SCALE_STEP);
});

fullscreenBtn.addEventListener("click", async () => {
    if (!document.fullscreenElement) {
        try {
            await canvas.requestFullscreen();
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
});

filterToggleBtn.addEventListener("click", () => {
    cycleFilter();
    filterToggleBtn.textContent = `Filter: ${filters[currentFilter].name}`;
});

saveStateBtn?.addEventListener("click", async () => {
    if (!saveStateController) return;
    const ok = await saveStateController.save();
    if (ok) {
        saveStateAvailable = true;
        updateSaveStateButtons();
    }
});

loadStateBtn?.addEventListener("click", async () => {
    if (!saveStateController) return;
    await saveStateController.load();
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

window.addEventListener("gamepadconnected", () => {
    updateConnectedGamepads();
    if (gamepadEnabled && running && !paused) {
        pollGamepad();
    }
});

window.addEventListener("gamepaddisconnected", () => {
    updateConnectedGamepads();
    resetGamepadState();
});

// Handle canvas resizing when entering/exiting fullscreen
document.addEventListener("fullscreenchange", () => {
    updateFullscreenButton();
    if (document.fullscreenElement) {
        // Entered fullscreen - calculate size to maintain aspect ratio
        const viewportWidth = window.innerWidth;
        const viewportHeight = window.innerHeight;
        const viewportAspect = viewportWidth / viewportHeight;
        const dpr = window.devicePixelRatio || 1;

        if (viewportAspect > NES_ASPECT_RATIO) {
            // Viewport is wider, fit to height
            canvas.style.height = "100vh";
            canvas.style.width = `${viewportHeight * NES_ASPECT_RATIO}px`;
            canvas.width = Math.round(viewportHeight * NES_ASPECT_RATIO * dpr);
            canvas.height = Math.round(viewportHeight * dpr);
        } else {
            // Viewport is taller, fit to width
            canvas.style.width = "100vw";
            canvas.style.height = `${viewportWidth / NES_ASPECT_RATIO}px`;
            canvas.width = Math.round(viewportWidth * dpr);
            canvas.height = Math.round((viewportWidth / NES_ASPECT_RATIO) * dpr);
        }
    } else {
        // Exited fullscreen - restore previous size
        updateCanvasSize(currentHeight);
    }
    
    // Update crosshair overlay size if it exists
    if (crosshair) {
        crosshair.updateCanvasSize();
    }
});
