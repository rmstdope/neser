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
