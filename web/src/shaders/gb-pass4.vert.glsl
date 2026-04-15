attribute vec2 a_position;
attribute vec2 a_texCoord;
uniform vec2 u_outputSize;
uniform vec2 u_sourceSize;
varying vec2 v_texCoord;
varying vec2 v_texel;
varying float v_shadowScaleFactor;

void main() {
    gl_Position = vec4(a_position, 0.0, 1.0);
    v_texCoord = a_texCoord * 1.0001;
    v_texel = vec2(1.0) / u_sourceSize;
    float sx = u_outputSize.x / 640.0;
    float sy = u_outputSize.y / 480.0;
    v_shadowScaleFactor = sqrt(sx * sy);
}
