attribute vec2 a_position;
attribute vec2 a_texCoord;
varying vec2 v_texCoord;
uniform vec2 u_sourceSize;

void main() {
    gl_Position = vec4(a_position, 0.0, 1.0);
    vec2 flipped = vec2(a_texCoord.x, 1.0 - a_texCoord.y);
    v_texCoord = flipped - vec2(0.5 / u_sourceSize.x, 0.0);
}
