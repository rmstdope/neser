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
