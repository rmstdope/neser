attribute vec2 a_position;
attribute vec2 a_texCoord;
uniform vec2 u_outputSize;
uniform vec2 u_sourceSize;
varying vec2 v_texCoord;
varying vec2 v_txCoord;
varying vec2 v_txToPx;
varying vec2 v_dotSizeInPx;

const float PIXEL_SIZE = 0.80;

void main() {
    gl_Position = vec4(a_position, 0.0, 1.0);
    v_texCoord = a_texCoord;
    v_txCoord = a_texCoord * u_sourceSize;
    v_txToPx = u_outputSize / u_sourceSize;
    v_dotSizeInPx = v_txToPx * PIXEL_SIZE;
}
