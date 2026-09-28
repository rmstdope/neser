#ifdef GL_FRAGMENT_PRECISION_HIGH
    precision highp float;
#else
    precision mediump float;
#endif
// GBA SP pass 1: the colours of the backlit GBA SP (AGS-101) screen.
// Ported from vendor/slang-shaders/handheld/shaders/color/sp101-color.slang
// Shader modified by Pokefan531; Color Mangler by hunterk. License: public domain.
// Colour profile 1 (sRGB), the preset default. Channels below zero are clamped before the output
// gamma, where upstream leaves pow() of a negative number undefined.
varying vec2 v_texCoord;
uniform sampler2D u_texture;

#define TARGET_GAMMA 2.2
#define DISPLAY_GAMMA 2.2
#define DARKEN_SCREEN 0.0
#define LUM 0.935

// Columns: what the input's red, green and blue each add to the output's red, green and blue.
const mat3 PROFILE = mat3(
    0.96, 0.0325, 0.001,
    0.11, 0.89, -0.03,
    -0.07, 0.0775, 1.029
);

void main() {
    vec3 screen = pow(texture2D(u_texture, v_texCoord).rgb, vec3(TARGET_GAMMA + DARKEN_SCREEN));
    screen = clamp(screen * LUM, 0.0, 1.0);
    screen = max(PROFILE * screen, 0.0);
    gl_FragColor = vec4(pow(screen, vec3(1.0 / DISPLAY_GAMMA)), 1.0);
}
