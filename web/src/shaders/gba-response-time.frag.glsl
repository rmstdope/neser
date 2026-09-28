#ifdef GL_FRAGMENT_PRECISION_HIGH
    precision highp float;
#else
    precision mediump float;
#endif
// LCD Grid pass 0: LCD response time, blending in the last seven frames.
// Ported from vendor/slang-shaders/motionblur/shaders/response-time.slang, based on the response
// time function from Harlequin's Game Boy and LCD shaders. License: GPL-2.0-or-later.
varying vec2 v_texCoord;
uniform sampler2D u_texture;
uniform sampler2D u_history1;
uniform sampler2D u_history2;
uniform sampler2D u_history3;
uniform sampler2D u_history4;
uniform sampler2D u_history5;
uniform sampler2D u_history6;
uniform sampler2D u_history7;

#define RESPONSE_TIME 0.333

void main() {
    vec3 rgb = texture2D(u_texture, v_texCoord).rgb;
    rgb += (texture2D(u_history1, v_texCoord).rgb - rgb) * RESPONSE_TIME;
    rgb += (texture2D(u_history2, v_texCoord).rgb - rgb) * pow(RESPONSE_TIME, 2.0);
    rgb += (texture2D(u_history3, v_texCoord).rgb - rgb) * pow(RESPONSE_TIME, 3.0);
    rgb += (texture2D(u_history4, v_texCoord).rgb - rgb) * pow(RESPONSE_TIME, 4.0);
    rgb += (texture2D(u_history5, v_texCoord).rgb - rgb) * pow(RESPONSE_TIME, 5.0);
    rgb += (texture2D(u_history6, v_texCoord).rgb - rgb) * pow(RESPONSE_TIME, 6.0);
    rgb += (texture2D(u_history7, v_texCoord).rgb - rgb) * pow(RESPONSE_TIME, 7.0);
    gl_FragColor = vec4(rgb, 1.0);
}
