// Vertex shader for every pass of the Game Boy Advance looks.
//
// The slang presets these looks come from address textures with (0,0) at the top left. Here every
// intermediate pass keeps the picture's top row at t = 0 (the row the frame upload puts there), so
// each pass reads its input exactly as upstream does; only the pass drawn to the screen, where GL
// puts y = +1 at the top, flips.
attribute vec2 a_position;
uniform float u_flipY;
varying vec2 v_texCoord;

void main() {
    gl_Position = vec4(a_position, 0.0, 1.0);
    float t = a_position.y * 0.5 + 0.5;
    v_texCoord = vec2(a_position.x * 0.5 + 0.5, u_flipY > 0.5 ? 1.0 - t : t);
}
