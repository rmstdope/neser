#ifdef GL_FRAGMENT_PRECISION_HIGH
    precision highp float;
#else
    precision mediump float;
#endif
// GBA SP pass 0: the reshade multiLUT shader with LUT 1 (reshade/shaders/LUT/64.png), the preset
// default. Ported from vendor/slang-shaders/reshade/shaders/LUT/multiLUT.slang (public domain).
// Upstream scales the texture coordinate by 1.0001 in its vertex shader; that is done here.
// Where both LUT slices are the same slice upstream divides zero by zero; that slice is used.
varying vec2 v_texCoord;
uniform sampler2D u_texture;
uniform sampler2D u_lut;

#define LUT_SIZE 64.0

void main() {
    vec4 imgColor = texture2D(u_texture, v_texCoord * 1.0001);
    float red = (imgColor.r * (LUT_SIZE - 1.0) + 0.4999) / (LUT_SIZE * LUT_SIZE);
    float green = (imgColor.g * (LUT_SIZE - 1.0) + 0.4999) / LUT_SIZE;
    float blue1 = (floor(imgColor.b * (LUT_SIZE - 1.0)) / LUT_SIZE) + red;
    float blue2 = (ceil(imgColor.b * (LUT_SIZE - 1.0)) / LUT_SIZE) + red;
    vec4 color1 = texture2D(u_lut, vec2(blue1, green));
    vec4 color2 = texture2D(u_lut, vec2(blue2, green));
    if (blue2 <= blue1) {
        gl_FragColor = color1;
        return;
    }
    float mixer = clamp(max((imgColor.b - blue1) / (blue2 - blue1), 0.0), 0.0, 32.0);
    gl_FragColor = (color1.z < 1.0) ? mix(color1, color2, mixer) : color1;
}
