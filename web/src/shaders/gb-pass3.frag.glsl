varying vec2 v_texCoord;
varying vec2 v_texel;
varying vec2 v_lowerBound;
varying vec2 v_upperBound;
uniform sampler2D u_texture;

void main() {
    float w0 = 0.13465834;
    float w1 = 0.13051534;
    float w2 = 0.11883558;
    float w3 = 0.10164547;
    float w4 = 0.08167444;
    vec4 col = texture2D(u_texture, clamp(v_texCoord, v_lowerBound, v_upperBound)) * w0;
    col.a += texture2D(u_texture, clamp(v_texCoord + vec2(0.0,  1.0 * v_texel.y), v_lowerBound, v_upperBound)).a * w1;
    col.a += texture2D(u_texture, clamp(v_texCoord + vec2(0.0, -1.0 * v_texel.y), v_lowerBound, v_upperBound)).a * w1;
    col.a += texture2D(u_texture, clamp(v_texCoord + vec2(0.0,  2.0 * v_texel.y), v_lowerBound, v_upperBound)).a * w2;
    col.a += texture2D(u_texture, clamp(v_texCoord + vec2(0.0, -2.0 * v_texel.y), v_lowerBound, v_upperBound)).a * w2;
    col.a += texture2D(u_texture, clamp(v_texCoord + vec2(0.0,  3.0 * v_texel.y), v_lowerBound, v_upperBound)).a * w3;
    col.a += texture2D(u_texture, clamp(v_texCoord + vec2(0.0, -3.0 * v_texel.y), v_lowerBound, v_upperBound)).a * w3;
    col.a += texture2D(u_texture, clamp(v_texCoord + vec2(0.0,  4.0 * v_texel.y), v_lowerBound, v_upperBound)).a * w4;
    col.a += texture2D(u_texture, clamp(v_texCoord + vec2(0.0, -4.0 * v_texel.y), v_lowerBound, v_upperBound)).a * w4;
    gl_FragColor = col;
}
