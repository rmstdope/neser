attribute vec2 a_position;
attribute vec2 a_texCoord;
varying vec2 v_texCoord;
varying vec2 v_pixelCoord;
uniform vec2 u_textureSize;

void main() {
    gl_Position = vec4(a_position, 0.0, 1.0);
    v_texCoord = a_texCoord;
    v_pixelCoord = a_texCoord * u_textureSize;
}
