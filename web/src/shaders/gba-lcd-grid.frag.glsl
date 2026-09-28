#ifdef GL_FRAGMENT_PRECISION_HIGH
    precision highp float;
#else
    precision mediump float;
#endif
// LCD Grid pass 1: cgwg's LCD grid (lcd-grid-v2), drawn at the viewport size.
// Ported from vendor/slang-shaders/handheld/console-border/shader-files/lcd-cgwg/lcd-grid-v2.slang
// with the gba-lcd-grid-v2 preset's parameters: gain 1, gamma 2.6, black level 0, ambient 0, BGR on,
// identity sub-pixel colours (so the sub-pixel colour matrix is left out), pixel aspect 1.
// u_videoScale is the viewport pixels per source pixel; the web computes it from the canvas
// (display/gba_lcd_grid.ts) where desktop uses the preset's fixed 4.
// texelFetchOffset becomes a nearest texture2D at the texel's centre (WebGL 1).
varying vec2 v_texCoord;
uniform sampler2D u_texture;
uniform vec2 u_sourceSize;
uniform vec2 u_outputSize;
uniform float u_videoScale;

#define LCD_GAMMA 2.6
#define OUTGAMMA 2.2

// integral of (1 - x^2 - x^4 + x^6)^2
float intsmearFuncX(float z) {
    float z2 = z * z;
    return z * (1.0 + z2 * (-2.0 / 3.0 + z2 * (-1.0 / 5.0 + z2 * (4.0 / 7.0
        + z2 * (-1.0 / 9.0 + z2 * (-2.0 / 11.0 + z2 * (1.0 / 13.0)))))));
}

// integral of (1 - 2x^4 + x^6)^2
float intsmearFuncY(float z) {
    float z2 = z * z;
    return z * (1.0 + z2 * (0.0 + z2 * (-4.0 / 5.0 + z2 * (2.0 / 7.0
        + z2 * (4.0 / 9.0 + z2 * (-4.0 / 11.0 + z2 * (1.0 / 13.0)))))));
}

float intsmearX(float x, float dx, float d) {
    float zl = clamp((x - dx * 0.5) / d, -1.0, 1.0);
    float zh = clamp((x + dx * 0.5) / d, -1.0, 1.0);
    return d * (intsmearFuncX(zh) - intsmearFuncX(zl)) / dx;
}

float intsmearY(float x, float dx, float d) {
    float zl = clamp((x - dx * 0.5) / d, -1.0, 1.0);
    float zh = clamp((x + dx * 0.5) / d, -1.0, 1.0);
    return d * (intsmearFuncY(zh) - intsmearFuncY(zl)) / dx;
}

vec3 fetch(vec2 texel) {
    vec3 c = texture2D(u_texture, (texel + 0.5) / u_sourceSize).rgb;
    return pow(c, vec3(LCD_GAMMA));
}

void main() {
    vec2 screenRatio = u_outputSize / u_sourceSize / u_videoScale;
    vec2 coord = vec2(0.5) + (v_texCoord - vec2(0.5)) * screenRatio;
    if (abs(coord.x - 0.5) >= 0.5 || abs(coord.y - 0.5) >= 0.5) {
        gl_FragColor = vec4(0.0, 0.0, 0.0, 1.0);
        return;
    }
    vec2 pos = coord * u_sourceSize - vec2(0.4999);
    vec2 tli = floor(pos);

    float subpix = (pos.x - tli.x) * 3.0;
    float rsubpix = 3.0 / u_videoScale;
    vec3 lcol = vec3(intsmearX(subpix + 1.0, rsubpix, 1.5),
                     intsmearX(subpix,       rsubpix, 1.5),
                     intsmearX(subpix - 1.0, rsubpix, 1.5));
    vec3 rcol = vec3(intsmearX(subpix - 2.0, rsubpix, 1.5),
                     intsmearX(subpix - 3.0, rsubpix, 1.5),
                     intsmearX(subpix - 4.0, rsubpix, 1.5));
    // BGR
    lcol = lcol.bgr;
    rcol = rcol.bgr;

    subpix = pos.y - tli.y;
    rsubpix = 1.0 / u_videoScale;
    float tcol = intsmearY(subpix,       rsubpix, 0.63);
    float bcol = intsmearY(subpix - 1.0, rsubpix, 0.63);

    vec3 topLeftColor     = fetch(tli)                   * lcol * vec3(tcol);
    vec3 bottomRightColor = fetch(tli + vec2(1.0, 1.0))  * rcol * vec3(bcol);
    vec3 bottomLeftColor  = fetch(tli + vec2(0.0, 1.0))  * lcol * vec3(bcol);
    vec3 topRightColor    = fetch(tli + vec2(1.0, 0.0))  * rcol * vec3(tcol);

    vec3 averageColor = topLeftColor + bottomRightColor + bottomLeftColor + topRightColor;
    gl_FragColor = vec4(pow(max(averageColor, 0.0), vec3(1.0 / OUTGAMMA)), 1.0);
}
