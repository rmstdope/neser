#ifdef GL_FRAGMENT_PRECISION_HIGH
    precision highp float;
#else
    precision mediump float;
#endif
// AGB-001 pass 0: a recreation of the original Game Boy Advance screen.
// Ported from vendor/slang-shaders/handheld/shaders/mgba/agb001.slang
// Author: endrift. License: MPL 2.0 (http://mozilla.org/MPL/2.0/).
// Drawn at 4x the source size; pass 1 is the stock shader, sampling linearly to the viewport.
// Upstream writes alpha 0.5, which desktop ignores; a browser canvas would blend it, so it is 1.
varying vec2 v_texCoord;
uniform sampler2D u_texture;
uniform vec2 u_sourceSize;

void main() {
    vec3 color = texture2D(u_texture, v_texCoord).rgb;
    color = pow(color * vec3(0.8, 0.8, 0.8), vec3(1.8, 1.8, 1.8)) + vec3(0.16, 0.16, 0.16);
    float x = floor(mod(v_texCoord.x * u_sourceSize.x * 4.0, 4.0));
    float y = floor(mod(v_texCoord.y * u_sourceSize.y * 4.0, 4.0));
    vec3 arrayX = x < 0.5 ? vec3(1.0, 0.2, 0.2)
        : x < 1.5 ? vec3(0.2, 1.0, 0.2)
        : x < 2.5 ? vec3(0.2, 0.2, 1.0)
        : vec3(0.4, 0.4, 0.4);
    vec3 arrayY = y < 2.5 ? vec3(1.0) : vec3(0.8);
    gl_FragColor = vec4(color * arrayX * arrayY, 1.0);
}
