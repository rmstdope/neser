varying vec2 v_texCoord;
varying vec2 v_blurUp;
varying vec2 v_blurDown;
varying vec2 v_blurRight;
varying vec2 v_blurLeft;
varying vec2 v_lowerBound;
varying vec2 v_upperBound;
uniform sampler2D u_texture;

const float BLENDING_MODE = 0.0;
const float ADJACENT_BLEND = 0.1755;

float blend_mod(float c) {
    float b = (c == 0.0) ? 1.0 : 0.0;
    return clamp(b + BLENDING_MODE, 0.0, 1.0);
}

void main() {
    vec4 col = texture2D(u_texture, v_texCoord);
    vec4 a1 = texture2D(u_texture, clamp(v_blurUp, v_lowerBound, v_upperBound));
    vec4 a2 = texture2D(u_texture, clamp(v_blurDown, v_lowerBound, v_upperBound));
    vec4 a3 = texture2D(u_texture, clamp(v_blurRight, v_lowerBound, v_upperBound));
    vec4 a4 = texture2D(u_texture, clamp(v_blurLeft, v_lowerBound, v_upperBound));
    col.a -= (
        (col.a - a1.a) + (col.a - a2.a) +
        (col.a - a3.a) + (col.a - a4.a)
    ) * ADJACENT_BLEND * blend_mod(col.a);
    gl_FragColor = col;
}
