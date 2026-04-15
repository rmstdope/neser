varying vec2 v_texCoord;
varying vec2 v_texel;
varying float v_shadowScaleFactor;
uniform sampler2D u_texture;       // blurred pass3 output (shadows)
uniform sampler2D u_gbPass1;       // pass1 output (foreground)
uniform sampler2D u_background;    // paper background
uniform sampler2D u_colorPalette;  // palette LUT
uniform vec2 u_sourceSize;
uniform vec2 u_gbPass1Size;

const float CONTRAST = 0.95;
const float SCREEN_LIGHT = 1.0;
const float PIXEL_OPACITY = 1.0;
const float BG_SMOOTHING = 0.75;
const float SHADOW_OPACITY = 0.55;
const float SHADOW_OFFSET_X = 1.0;
const float SHADOW_OFFSET_Y = 1.0;
const float SHADOW_ENABLE = 1.0;
const float SCREEN_OFFSET_X = 0.0;
const float SCREEN_OFFSET_Y = 0.0;
const float PALETTE = 0.0;

vec4 get_bg_color() {
    if (PALETTE < 0.5) return texture2D(u_colorPalette, vec2(0.25, 0.5));
    if (PALETTE < 1.5) return vec4(0.651, 0.675, 0.518, 1.0);
    if (PALETTE < 2.5) return vec4(0.737, 0.737, 0.737, 1.0);
    if (PALETTE < 3.5) return vec4(1.0, 1.0, 1.0, 1.0);
    if (PALETTE < 4.5) return vec4(0.627, 0.667, 0.024, 1.0);
    if (PALETTE < 5.5) return vec4(0.027, 0.729, 0.369, 1.0);
    return vec4(0.027, 0.729, 0.608, 1.0);
}

void main() {
    vec2 tex = floor(u_gbPass1Size * v_texCoord);
    tex = (tex + 0.5) / u_gbPass1Size;
    vec4 bg_c = get_bg_color();
    float sha = CONTRAST * SHADOW_OPACITY * SHADOW_ENABLE;
    vec2 s_off = vec2(SHADOW_OFFSET_X * v_texel.x * v_shadowScaleFactor,
                      SHADOW_OFFSET_Y * v_texel.y * v_shadowScaleFactor);
    vec2 sc_off = vec2((SCREEN_OFFSET_X - 1.0) * v_texel.x,
                       (SCREEN_OFFSET_Y - 1.0) * v_texel.y);
    vec4 fg = texture2D(u_gbPass1, tex - sc_off);
    vec4 bg = texture2D(u_background, v_texCoord);
    vec4 sh = texture2D(u_texture, v_texCoord - (s_off + sc_off));
    fg *= bg_c;
    float bg_test = (fg.a > 0.0) ? 1.0 : 0.0;
    bg -= (bg - 0.5) * BG_SMOOTHING * bg_test;
    bg.rgb = clamp(vec3(
        bg_c.r + mix(-1.0, 1.0, bg.r),
        bg_c.g + mix(-1.0, 1.0, bg.g),
        bg_c.b + mix(-1.0, 1.0, bg.b)
    ), 0.0, 1.0);
    vec4 out_c = sh * sh.a * sha + bg * (1.0 - sh.a * sha);
    out_c = fg * fg.a * CONTRAST +
            out_c * (SCREEN_LIGHT - fg.a * CONTRAST * PIXEL_OPACITY);
    gl_FragColor = out_c;
}
