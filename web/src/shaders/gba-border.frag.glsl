#ifdef GL_FRAGMENT_PRECISION_HIGH
    precision highp float;
#else
    precision mediump float;
#endif
// LCD Grid pass 3: the Game Boy Advance console drawn around the picture.
// Ported from vendor/slang-shaders/handheld/console-border/shader-files/gb-pass-5.slang with the
// border art resources/gba-border-square-4x.png (3200x1600, 4x) at border brightness 1 and no
// offset. The art is drawn at u_videoScale viewport pixels per 1x art pixel, as the picture is.
// Outside the art the edge (black) is repeated. The page gets alpha 1.
varying vec2 v_texCoord;
uniform sampler2D u_texture;
uniform sampler2D u_border;
uniform vec2 u_outputSize;
uniform float u_videoScale;

void main() {
    vec2 borderScale = vec2(800.0, 400.0) * u_videoScale;
    vec2 texBorder = vec2(0.5) + (v_texCoord - vec2(0.5)) * u_outputSize / borderScale;
    vec4 frame = texture2D(u_texture, v_texCoord);
    vec4 border = texture2D(u_border, texBorder);
    gl_FragColor = vec4(mix(frame.rgb, border.rgb, border.a), 1.0);
}
