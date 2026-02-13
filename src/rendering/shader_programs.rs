//! Custom GLSL shader programs for NES rendering filters.
//!
//! This module contains the shader source code and utilities for compiling
//! and managing custom OpenGL shaders. These shaders replace the librashader
//! dependency and provide the same visual filters: stock, CRT, NTSC, and smooth.

use gl::types::{GLenum, GLuint};
use std::ffi::CString;
use std::ptr;

/// Common vertex shader used by all single-pass filters.
/// Updated to GLSL 1.50 for OpenGL 3.2 core compatibility.
pub const VERTEX_SHADER_SOURCE: &str = r#"
    #version 150 core
    in vec2 a_position;
    in vec2 a_texCoord;
    out vec2 v_texCoord;
    out vec2 v_pixelCoord;
    uniform vec2 u_textureSize;

    void main() {
        gl_Position = vec4(a_position, 0.0, 1.0);
        v_texCoord = a_texCoord;
        v_pixelCoord = a_texCoord * u_textureSize;
    }
"#;

/// Stock/None filter - simple pass-through with no effects.
pub const STOCK_FRAGMENT_SHADER_SOURCE: &str = r#"
    #version 150 core
    in vec2 v_texCoord;
    out vec4 fragColor;
    uniform sampler2D u_texture;

    void main() {
        fragColor = texture(u_texture, v_texCoord);
    }
"#;

/// Smooth filter - uses GPU's built-in linear filtering.
/// This is a simple pass-through shader that relies on GL_LINEAR texture filtering.
/// Mipmaps are generated for the input texture in gl_backend.rs, enabling trilinear
/// filtering when GL_LINEAR_MIPMAP_LINEAR is used.
pub const SMOOTH_FRAGMENT_SHADER_SOURCE: &str = r#"
    #version 150 core
    in vec2 v_texCoord;
    out vec4 fragColor;
    uniform sampler2D u_texture;

    void main() {
        // Simple sampling - the smoothing is done by GL_LINEAR filtering
        fragColor = texture(u_texture, v_texCoord);
    }
"#;

/// CRT filter - scanlines, shadow mask, bloom, and screen warp effects.
/// Ported from the web frontend implementation.
pub const CRT_FRAGMENT_SHADER_SOURCE: &str = r#"
    #version 150 core
    in vec2 v_texCoord;
    out vec4 fragColor;
    uniform sampler2D u_texture;
    uniform vec2 u_sourceSize;
    uniform vec2 u_outputSize;
    uniform float u_hardScan;
    uniform float u_hardPix;
    uniform float u_warpX;
    uniform float u_warpY;
    uniform float u_maskDark;
    uniform float u_maskLight;
    uniform float u_scaleInLinearGamma;
    uniform float u_shadowMask;
    uniform float u_brightBoost;
    uniform float u_hardBloomScan;
    uniform float u_hardBloomPix;
    uniform float u_bloomAmount;
    uniform float u_shape;

    #define DO_BLOOM 1

    float ToLinear1(float c) {
        if (u_scaleInLinearGamma == 0.0) {
            return c;
        }
        return (c <= 0.04045) ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4);
    }

    vec3 ToLinear(vec3 c) {
        if (u_scaleInLinearGamma == 0.0) {
            return c;
        }
        return vec3(ToLinear1(c.r), ToLinear1(c.g), ToLinear1(c.b));
    }

    float ToSrgb1(float c) {
        if (u_scaleInLinearGamma == 0.0) {
            return c;
        }
        return (c < 0.0031308) ? c * 12.92 : 1.055 * pow(c, 0.41666) - 0.055;
    }

    vec3 ToSrgb(vec3 c) {
        if (u_scaleInLinearGamma == 0.0) {
            return c;
        }
        return vec3(ToSrgb1(c.r), ToSrgb1(c.g), ToSrgb1(c.b));
    }

    vec3 Fetch(vec2 pos, vec2 off) {
        pos = (floor(pos * u_sourceSize + off) + vec2(0.5, 0.5)) / u_sourceSize;
        return ToLinear(u_brightBoost * texture(u_texture, pos.xy).rgb);
    }

    vec2 Dist(vec2 pos) {
        pos = pos * u_sourceSize;
        return -((pos - floor(pos)) - vec2(0.5));
    }

    float Gaus(float pos, float scale) {
        return exp2(scale * pow(abs(pos), u_shape));
    }

    vec3 Horz3(vec2 pos, float off) {
        vec3 b = Fetch(pos, vec2(-1.0, off));
        vec3 c = Fetch(pos, vec2(0.0, off));
        vec3 d = Fetch(pos, vec2(1.0, off));
        float dst = Dist(pos).x;

        float scale = u_hardPix;
        float wb = Gaus(dst - 1.0, scale);
        float wc = Gaus(dst + 0.0, scale);
        float wd = Gaus(dst + 1.0, scale);

        return (b * wb + c * wc + d * wd) / (wb + wc + wd);
    }

    vec3 Horz5(vec2 pos, float off) {
        vec3 a = Fetch(pos, vec2(-2.0, off));
        vec3 b = Fetch(pos, vec2(-1.0, off));
        vec3 c = Fetch(pos, vec2(0.0, off));
        vec3 d = Fetch(pos, vec2(1.0, off));
        vec3 e = Fetch(pos, vec2(2.0, off));

        float dst = Dist(pos).x;
        float scale = u_hardPix;
        float wa = Gaus(dst - 2.0, scale);
        float wb = Gaus(dst - 1.0, scale);
        float wc = Gaus(dst + 0.0, scale);
        float wd = Gaus(dst + 1.0, scale);
        float we = Gaus(dst + 2.0, scale);

        return (a * wa + b * wb + c * wc + d * wd + e * we) / (wa + wb + wc + wd + we);
    }

    vec3 Horz7(vec2 pos, float off) {
        vec3 a = Fetch(pos, vec2(-3.0, off));
        vec3 b = Fetch(pos, vec2(-2.0, off));
        vec3 c = Fetch(pos, vec2(-1.0, off));
        vec3 d = Fetch(pos, vec2(0.0, off));
        vec3 e = Fetch(pos, vec2(1.0, off));
        vec3 f = Fetch(pos, vec2(2.0, off));
        vec3 g = Fetch(pos, vec2(3.0, off));

        float dst = Dist(pos).x;
        float scale = u_hardBloomPix;
        float wa = Gaus(dst - 3.0, scale);
        float wb = Gaus(dst - 2.0, scale);
        float wc = Gaus(dst - 1.0, scale);
        float wd = Gaus(dst + 0.0, scale);
        float we = Gaus(dst + 1.0, scale);
        float wf = Gaus(dst + 2.0, scale);
        float wg = Gaus(dst + 3.0, scale);

        return (a * wa + b * wb + c * wc + d * wd + e * we + f * wf + g * wg) /
            (wa + wb + wc + wd + we + wf + wg);
    }

    float Scan(vec2 pos, float off) {
        float dst = Dist(pos).y;
        return Gaus(dst + off, u_hardScan);
    }

    float BloomScan(vec2 pos, float off) {
        float dst = Dist(pos).y;
        return Gaus(dst + off, u_hardBloomScan);
    }

    vec3 Tri(vec2 pos) {
        vec3 a = Horz3(pos, -1.0);
        vec3 b = Horz5(pos, 0.0);
        vec3 c = Horz3(pos, 1.0);

        float wa = Scan(pos, -1.0);
        float wb = Scan(pos, 0.0);
        float wc = Scan(pos, 1.0);

        return a * wa + b * wb + c * wc;
    }

    vec3 Bloom(vec2 pos) {
        vec3 a = Horz5(pos, -2.0);
        vec3 b = Horz7(pos, -1.0);
        vec3 c = Horz7(pos, 0.0);
        vec3 d = Horz7(pos, 1.0);
        vec3 e = Horz5(pos, 2.0);

        float wa = BloomScan(pos, -2.0);
        float wb = BloomScan(pos, -1.0);
        float wc = BloomScan(pos, 0.0);
        float wd = BloomScan(pos, 1.0);
        float we = BloomScan(pos, 2.0);

        return a * wa + b * wb + c * wc + d * wd + e * we;
    }

    vec2 Warp(vec2 pos) {
        pos = pos * 2.0 - 1.0;
        pos *= vec2(1.0 + (pos.y * pos.y) * u_warpX, 1.0 + (pos.x * pos.x) * u_warpY);
        return pos * 0.5 + 0.5;
    }

    vec3 Mask(vec2 pos) {
        vec3 mask = vec3(u_maskDark);

        if (u_shadowMask == 1.0) {
            float line = u_maskLight;
            float odd = 0.0;

            if (fract(pos.x * 0.166666666) < 0.5) odd = 1.0;
            if (fract((pos.y + odd) * 0.5) < 0.5) line = u_maskDark;

            pos.x = fract(pos.x * 0.333333333);

            if (pos.x < 0.333) mask.r = u_maskLight;
            else if (pos.x < 0.666) mask.g = u_maskLight;
            else mask.b = u_maskLight;
            mask *= line;
        } else if (u_shadowMask == 2.0) {
            pos.x = fract(pos.x * 0.333333333);

            if (pos.x < 0.333) mask.r = u_maskLight;
            else if (pos.x < 0.666) mask.g = u_maskLight;
            else mask.b = u_maskLight;
        } else if (u_shadowMask == 3.0) {
            pos.x += pos.y * 3.0;
            pos.x = fract(pos.x * 0.166666666);

            if (pos.x < 0.333) mask.r = u_maskLight;
            else if (pos.x < 0.666) mask.g = u_maskLight;
            else mask.b = u_maskLight;
        } else if (u_shadowMask == 4.0) {
            pos = floor(pos * vec2(1.0, 0.5));
            pos.x += pos.y * 3.0;
            pos.x = fract(pos.x * 0.166666666);

            if (pos.x < 0.333) mask.r = u_maskLight;
            else if (pos.x < 0.666) mask.g = u_maskLight;
            else mask.b = u_maskLight;
        }

        return mask;
    }

    void main() {
        vec2 pos = Warp(v_texCoord);
        if (pos.x < 0.0 || pos.x > 1.0 || pos.y < 0.0 || pos.y > 1.0) {
            fragColor = vec4(0.0, 0.0, 0.0, 1.0);
            return;
        }
        vec3 outColor = Tri(pos);

    #ifdef DO_BLOOM
        outColor.rgb += Bloom(pos) * u_bloomAmount;
    #endif

        if (u_shadowMask > 0.0) {
            outColor.rgb *= Mask(v_texCoord * u_outputSize * 1.000001);
        }

        fragColor = vec4(ToSrgb(outColor.rgb), 1.0);
    }
"#;

/// NTSC Pass 1 vertex shader - encodes RGB to YIQ with chroma modulation.
pub const NTSC_PASS1_VERTEX_SHADER_SOURCE: &str = r#"
    #version 150 core
    in vec2 a_position;
    in vec2 a_texCoord;
    out vec2 v_texCoord;
    out vec2 v_pixNo;
    uniform vec2 u_outputSize;

    void main() {
        gl_Position = vec4(a_position, 0.0, 1.0);
        v_texCoord = a_texCoord;
        v_pixNo = a_texCoord * u_outputSize;
    }
"#;

/// NTSC Pass 1 fragment shader - RGB to YIQ encoding with chroma modulation.
pub const NTSC_PASS1_FRAGMENT_SHADER_SOURCE: &str = r#"
    #version 150 core
    in vec2 v_texCoord;
    in vec2 v_pixNo;
    out vec4 fragColor;
    uniform sampler2D u_texture;
    uniform float u_frameCount;
    uniform float u_chromaEncode;

    #define PI 3.14159265
    #define CHROMA_MOD_FREQ (PI / 3.0)
    #define SATURATION 1.0
    #define BRIGHTNESS 1.0
    #define ARTIFACTING 1.0
    #define FRINGING 1.0

    const mat3 mix_mat = mat3(
      BRIGHTNESS, FRINGING, FRINGING,
      ARTIFACTING, 2.0 * SATURATION, 0.0,
      ARTIFACTING, 0.0, 2.0 * SATURATION
    );

    const mat3 yiq_mat = mat3(
      0.2989, 0.5870, 0.1140,
      0.5959, -0.2744, -0.3216,
      0.2115, -0.5229, 0.3114
    );

    vec3 rgb2yiq(vec3 col) {
        return col * yiq_mat;
    }

    void main() {
        vec3 col = texture(u_texture, v_texCoord).rgb;
        vec3 yiq = rgb2yiq(col);

        float chroma_phase = 0.6667 * PI * (mod(v_pixNo.y, 3.0) + u_frameCount);
        float mod_phase = chroma_phase + v_pixNo.x * CHROMA_MOD_FREQ;
        float i_mod = cos(mod_phase);
        float q_mod = sin(mod_phase);

        yiq.yz *= vec2(i_mod, q_mod); // Modulate.
        yiq *= mix_mat; // Cross-talk.
        yiq.yz *= vec2(i_mod, q_mod); // Demodulate.

        // Optional encoding for UNORM render targets: pack I/Q into 0..1
        yiq.yz = mix(yiq.yz, yiq.yz * 0.5 + 0.5, u_chromaEncode);

        fragColor = vec4(yiq, 1.0);
    }
"#;

/// NTSC Pass 2 vertex shader - prepares for horizontal filtering.
pub const NTSC_PASS2_VERTEX_SHADER_SOURCE: &str = r#"
    #version 150 core
    in vec2 a_position;
    in vec2 a_texCoord;
    out vec2 v_texCoord;
    uniform vec2 u_sourceSize;

    void main() {
        gl_Position = vec4(a_position, 0.0, 1.0);
        // No Y-flip needed since we're rendering to a texture, not the screen
        v_texCoord = a_texCoord - vec2(0.5 / u_sourceSize.x, 0.0);
    }
"#;

/// NTSC Pass 2 fragment shader - YIQ to RGB decoding with filtering.
pub const NTSC_PASS2_FRAGMENT_SHADER_SOURCE: &str = r#"
    #version 150 core
    in vec2 v_texCoord;
    out vec4 fragColor;
    uniform sampler2D u_texture;
    uniform vec2 u_sourceSize;
    uniform float u_chromaEncode;
    uniform float u_chromaSum;

    #define TAPS 24
    #define NTSC_CRT_GAMMA 2.5
    #define NTSC_MONITOR_GAMMA 2.0

    float lumaTap(int i) {
        if (i == 0) return -0.000012020;
        if (i == 1) return -0.000022146;
        if (i == 2) return -0.000013155;
        if (i == 3) return -0.000012020;
        if (i == 4) return -0.000049979;
        if (i == 5) return -0.000113940;
        if (i == 6) return -0.000122150;
        if (i == 7) return -0.000005612;
        if (i == 8) return 0.000170516;
        if (i == 9) return 0.000237199;
        if (i == 10) return 0.000169640;
        if (i == 11) return 0.000285688;
        if (i == 12) return 0.000984574;
        if (i == 13) return 0.002018683;
        if (i == 14) return 0.002002275;
        if (i == 15) return -0.000909882;
        if (i == 16) return -0.007049081;
        if (i == 17) return -0.013222860;
        if (i == 18) return -0.012606931;
        if (i == 19) return 0.002460860;
        if (i == 20) return 0.035868225;
        if (i == 21) return 0.084016453;
        if (i == 22) return 0.135563500;
        if (i == 23) return 0.175261268;
        return 0.190176552;
    }

    float chromaTap(int i) {
        if (i == 0) return -0.000118847;
        if (i == 1) return -0.000271306;
        if (i == 2) return -0.000502642;
        if (i == 3) return -0.000930833;
        if (i == 4) return -0.001451013;
        if (i == 5) return -0.002064744;
        if (i == 6) return -0.002700432;
        if (i == 7) return -0.003241276;
        if (i == 8) return -0.003524948;
        if (i == 9) return -0.003350284;
        if (i == 10) return -0.002491729;
        if (i == 11) return -0.000721149;
        if (i == 12) return 0.002164659;
        if (i == 13) return 0.006313635;
        if (i == 14) return 0.011789103;
        if (i == 15) return 0.018545660;
        if (i == 16) return 0.026414396;
        if (i == 17) return 0.035100710;
        if (i == 18) return 0.044196567;
        if (i == 19) return 0.053207202;
        if (i == 20) return 0.061590275;
        if (i == 21) return 0.068803602;
        if (i == 22) return 0.074356193;
        if (i == 23) return 0.077856564;
        return 0.079052396;
    }

    const mat3 yiq2rgb_mat = mat3(
        1.0, 0.956, 0.6210,
        1.0, -0.2720, -0.6474,
        1.0, -1.1060, 1.7046
    );

    vec3 yiq2rgb(vec3 yiq) {
        return yiq * yiq2rgb_mat;
    }

    vec3 fetch_offset(float offset, float one_x) {
        return texture(u_texture, v_texCoord + vec2(offset * one_x, 0.0)).xyz;
    }

    void main() {
        float one_x = 1.0 / u_sourceSize.x;
        vec3 signal = vec3(0.0);
        for (int i = 0; i < TAPS; i++) {
            float offset = float(i);
            vec3 sums = fetch_offset(offset - float(TAPS), one_x) +
                fetch_offset(float(TAPS) - offset, one_x);
            float luma = lumaTap(i);
            float chroma = chromaTap(i);
            signal += sums * vec3(luma, chroma, chroma);
        }
        signal += texture(u_texture, v_texCoord).xyz *
            vec3(lumaTap(TAPS), chromaTap(TAPS), chromaTap(TAPS));

        // Optional decoding for UNORM render targets
        signal.yz = mix(signal.yz, signal.yz * 2.0 - vec2(u_chromaSum), u_chromaEncode);

        vec3 rgb = yiq2rgb(signal);
        fragColor = vec4(pow(rgb, vec3(NTSC_CRT_GAMMA / NTSC_MONITOR_GAMMA)), 1.0);
    }
"#;

/// Compile a shader from source.
pub fn compile_shader(shader_type: GLenum, source: &str) -> Result<GLuint, String> {
    unsafe {
        let shader = gl::CreateShader(shader_type);
        if shader == 0 {
            return Err("Failed to create shader".to_string());
        }

        let c_str = CString::new(source).map_err(|e| format!("CString error: {}", e))?;
        gl::ShaderSource(shader, 1, &c_str.as_ptr(), ptr::null());
        gl::CompileShader(shader);

        // Check compilation status
        let mut success: gl::types::GLint = 0;
        gl::GetShaderiv(shader, gl::COMPILE_STATUS, &mut success);

        if success == 0 {
            let mut len: gl::types::GLint = 0;
            gl::GetShaderiv(shader, gl::INFO_LOG_LENGTH, &mut len);

            let mut buffer = vec![0u8; len as usize];
            gl::GetShaderInfoLog(shader, len, ptr::null_mut(), buffer.as_mut_ptr() as *mut _);

            let error = String::from_utf8_lossy(&buffer).to_string();
            gl::DeleteShader(shader);
            return Err(format!("Shader compilation failed: {}", error));
        }

        Ok(shader)
    }
}

/// Link a shader program from vertex and fragment shaders.
pub fn link_program(vertex_shader: GLuint, fragment_shader: GLuint) -> Result<GLuint, String> {
    unsafe {
        let program = gl::CreateProgram();
        if program == 0 {
            return Err("Failed to create program".to_string());
        }

        gl::AttachShader(program, vertex_shader);
        gl::AttachShader(program, fragment_shader);
        gl::LinkProgram(program);

        // Check link status
        let mut success: gl::types::GLint = 0;
        gl::GetProgramiv(program, gl::LINK_STATUS, &mut success);

        if success == 0 {
            let mut len: gl::types::GLint = 0;
            gl::GetProgramiv(program, gl::INFO_LOG_LENGTH, &mut len);

            let mut buffer = vec![0u8; len as usize];
            gl::GetProgramInfoLog(program, len, ptr::null_mut(), buffer.as_mut_ptr() as *mut _);

            let error = String::from_utf8_lossy(&buffer).to_string();
            gl::DeleteProgram(program);
            return Err(format!("Program linking failed: {}", error));
        }

        Ok(program)
    }
}

/// Compile and link a complete shader program.
pub fn create_shader_program(
    vertex_source: &str,
    fragment_source: &str,
) -> Result<GLuint, String> {
    let vertex_shader = compile_shader(gl::VERTEX_SHADER, vertex_source)?;
    let fragment_shader = compile_shader(gl::FRAGMENT_SHADER, fragment_source)?;

    let program = link_program(vertex_shader, fragment_shader)?;

    // Clean up shaders after linking
    unsafe {
        gl::DeleteShader(vertex_shader);
        gl::DeleteShader(fragment_shader);
    }

    Ok(program)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shader_sources_not_empty() {
        assert!(!VERTEX_SHADER_SOURCE.is_empty());
        assert!(!STOCK_FRAGMENT_SHADER_SOURCE.is_empty());
        assert!(!SMOOTH_FRAGMENT_SHADER_SOURCE.is_empty());
        assert!(!CRT_FRAGMENT_SHADER_SOURCE.is_empty());
        assert!(!NTSC_PASS1_VERTEX_SHADER_SOURCE.is_empty());
        assert!(!NTSC_PASS1_FRAGMENT_SHADER_SOURCE.is_empty());
        assert!(!NTSC_PASS2_VERTEX_SHADER_SOURCE.is_empty());
        assert!(!NTSC_PASS2_FRAGMENT_SHADER_SOURCE.is_empty());
    }

    #[test]
    fn test_shader_sources_contain_main() {
        assert!(STOCK_FRAGMENT_SHADER_SOURCE.contains("void main()"));
        assert!(SMOOTH_FRAGMENT_SHADER_SOURCE.contains("void main()"));
        assert!(CRT_FRAGMENT_SHADER_SOURCE.contains("void main()"));
        assert!(NTSC_PASS1_FRAGMENT_SHADER_SOURCE.contains("void main()"));
        assert!(NTSC_PASS2_FRAGMENT_SHADER_SOURCE.contains("void main()"));
    }

    #[test]
    fn test_shader_version_150() {
        assert!(VERTEX_SHADER_SOURCE.contains("#version 150"));
        assert!(STOCK_FRAGMENT_SHADER_SOURCE.contains("#version 150"));
        assert!(CRT_FRAGMENT_SHADER_SOURCE.contains("#version 150"));
        assert!(NTSC_PASS1_VERTEX_SHADER_SOURCE.contains("#version 150"));
        assert!(NTSC_PASS1_FRAGMENT_SHADER_SOURCE.contains("#version 150"));
        assert!(NTSC_PASS2_VERTEX_SHADER_SOURCE.contains("#version 150"));
        assert!(NTSC_PASS2_FRAGMENT_SHADER_SOURCE.contains("#version 150"));
    }
}
