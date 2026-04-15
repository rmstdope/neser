varying vec2 v_texCoord;
varying vec2 v_txCoord;
varying vec2 v_txToPx;
varying vec2 v_dotSizeInPx;
uniform sampler2D u_texture;
uniform sampler2D u_prevFrame;
uniform sampler2D u_colorPalette;
uniform vec2 u_sourceSize;
uniform vec2 u_outputSize;

// Hardcoded defaults from gb-params.inc + gameboy.slangp
const float PIXEL_SIZE = 0.80;
const float PIXEL_SOFTNESS = 1.0;
const float PIXEL_SHAPE = 1.0;
const float SHARPENING_AMOUNT = 1.0;
const float RESPONSE_TIME = 0.33;
const float GREY_BALANCE = 3.0;
const float BASELINE_ALPHA = 0.10;
const float COLOR_TOGGLE = 0.0;
const float PALETTE = 0.0;
// Pre-computed grey balance compensation (sigmoid mode, default params)
const float AA_COMPENSATION = 1.6;

float intersect_line(float px_s, float px_e, float dot_s, float dot_e) {
    return max(min(px_e, dot_e) - max(px_s, dot_s), 0.0);
}

float intersect_rect(vec4 px, vec4 rect) {
    vec2 bl = max(px.xy, rect.xy);
    vec2 tr = min(px.zw, rect.zw);
    vec2 c = max(tr - bl, vec2(0.0));
    return c.x * c.y;
}

void main() {
    vec3 fg_source = texture2D(u_texture, v_texCoord).rgb;

    // Response time: current + 1 previous frame
    vec3 curr = abs(vec3(1.0) - texture2D(u_texture, v_texCoord).rgb);
    vec3 prev = abs(vec3(1.0) - texture2D(u_prevFrame, v_texCoord).rgb);

    // Geometric dot intersection (fullscreen mode)
    vec2 tx_i = floor(v_txCoord);
    vec2 tx_f = v_txCoord - tx_i;
    vec2 pc = (tx_f - 0.5) * v_txToPx;
    vec4 pr = vec4(pc - v_txToPx * 0.5, pc + v_txToPx * 0.5);
    vec4 dr = vec4(-v_dotSizeInPx * 0.5, v_dotSizeInPx * 0.5);

    // Rectangular coverage with sigmoid sharpening
    float xc = intersect_line(pr.x, pr.z, dr.x, dr.z) / v_txToPx.x;
    float yc = intersect_line(pr.y, pr.w, dr.y, dr.w) / v_txToPx.y;
    float ss = 10.0 / max(PIXEL_SOFTNESS, 0.001);
    float xs = 1.0 / (1.0 + exp(-ss * (xc - 0.5)));
    float ys = 1.0 / (1.0 + exp(-ss * (yc - 0.5)));
    float rect_cov = mix(xc * yc, xs * ys, SHARPENING_AMOUNT);

    // Circular coverage with sigmoid
    float cl = intersect_rect(pr, dr) / (v_txToPx.x * v_txToPx.y);
    float cs = 1.0 / (1.0 + exp(-ss * (cl - 0.5)));
    float circ_cov = mix(cl, cs, SHARPENING_AMOUNT);

    float is_on_dot = mix(circ_cov, rect_cov, PIXEL_SHAPE);

    // Response time blending
    vec3 input_rgb = curr;
    input_rgb += (prev - input_rgb) * RESPONSE_TIME;

    // Brightness (simple mode)
    float brightness = input_rgb.r + input_rgb.g + input_rgb.b;
    float grey_adj = GREY_BALANCE / AA_COMPENSATION;
    float alpha = brightness / grey_adj + BASELINE_ALPHA;

    // Foreground color
    vec3 fg_color = texture2D(u_colorPalette, vec2(0.75, 0.5)).rgb;
    vec4 out_color;
    if (COLOR_TOGGLE < 0.5)
        out_color = vec4(fg_color, alpha);
    else
        out_color = vec4(fg_source, alpha);

    out_color.a *= is_on_dot;
    gl_FragColor = out_color;
}
