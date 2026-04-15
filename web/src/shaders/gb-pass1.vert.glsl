attribute vec2 a_position;
attribute vec2 a_texCoord;
uniform vec2 u_sourceSize;
uniform vec2 u_outputSize;
varying vec2 v_texCoord;
varying vec2 v_blurUp;
varying vec2 v_blurDown;
varying vec2 v_blurRight;
varying vec2 v_blurLeft;
varying vec2 v_lowerBound;
varying vec2 v_upperBound;

void main() {
    gl_Position = vec4(a_position, 0.0, 1.0);
    v_texCoord = a_texCoord * 1.0001;
    vec2 texel = vec2(1.0) / u_sourceSize;
    v_blurDown  = v_texCoord + vec2(0.0, texel.y);
    v_blurUp    = v_texCoord + vec2(0.0, -texel.y);
    v_blurRight = v_texCoord + vec2(texel.x, 0.0);
    v_blurLeft  = v_texCoord + vec2(-texel.x, 0.0);
    v_lowerBound = vec2(0.0);
    v_upperBound = texel * (u_outputSize - vec2(2.0));
}
