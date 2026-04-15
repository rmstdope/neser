attribute vec2 a_position;
attribute vec2 a_texCoord;
uniform vec2 u_sourceSize;
uniform vec2 u_outputSize;
varying vec2 v_texCoord;
varying vec2 v_texel;
varying vec2 v_lowerBound;
varying vec2 v_upperBound;

void main() {
    gl_Position = vec4(a_position, 0.0, 1.0);
    v_texCoord = a_texCoord * 1.0001;
    v_texel = vec2(1.0) / u_sourceSize;
    v_lowerBound = vec2(0.0);
    v_upperBound = v_texel * (u_outputSize - vec2(1.0));
}
