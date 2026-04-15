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
