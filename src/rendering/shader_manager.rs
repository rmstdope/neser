//! Custom shader manager for NES rendering filters.
//!
//! This module manages shader programs for visual filters without using librashaders.
//! It provides stock, CRT, NTSC, and smooth filters using custom OpenGL shader programs.

use crate::rendering::shader_programs::*;
use gl::types::GLuint;
use std::ffi::CString;
use std::ptr;

/// CRT filter parameters
#[derive(Debug, Clone)]
pub struct CrtParams {
    pub hard_scan: f32,
    pub hard_pix: f32,
    pub warp_x: f32,
    pub warp_y: f32,
    pub mask_dark: f32,
    pub mask_light: f32,
    pub scale_in_linear_gamma: f32,
    pub shadow_mask: f32,
    pub bright_boost: f32,
    pub hard_bloom_scan: f32,
    pub hard_bloom_pix: f32,
    pub bloom_amount: f32,
    pub shape: f32,
}

impl Default for CrtParams {
    fn default() -> Self {
        CrtParams {
            hard_scan: -8.0,
            hard_pix: -3.0,
            warp_x: 0.031,
            warp_y: 0.041,
            mask_dark: 0.5,
            mask_light: 1.5,
            scale_in_linear_gamma: 1.0,
            shadow_mask: 3.0,
            bright_boost: 1.0,
            hard_bloom_scan: -2.0,
            hard_bloom_pix: -1.5,
            bloom_amount: 0.15,
            shape: 2.0,
        }
    }
}

/// Filter type enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterType {
    Stock,
    Crt,
    Ntsc,
    Smooth,
}

impl FilterType {
    pub fn name(&self) -> &'static str {
        match self {
            FilterType::Stock => "None",
            FilterType::Crt => "CRT",
            FilterType::Ntsc => "NTSC",
            FilterType::Smooth => "Smooth",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "stock" | "none" => Some(FilterType::Stock),
            "crt" => Some(FilterType::Crt),
            "ntsc" => Some(FilterType::Ntsc),
            "smooth" => Some(FilterType::Smooth),
            _ => None,
        }
    }
}

const FILTER_ORDER: [FilterType; 4] = [
    FilterType::Stock,
    FilterType::Ntsc,
    FilterType::Crt,
    FilterType::Smooth,
];

/// Shader program state for single-pass filters
struct ShaderProgram {
    program: GLuint,
    // Attribute locations
    a_position: gl::types::GLint,
    a_tex_coord: gl::types::GLint,
    // Uniform locations
    u_texture: gl::types::GLint,
    u_texture_size: gl::types::GLint,
    u_source_size: gl::types::GLint,
    u_output_size: gl::types::GLint,
    // CRT-specific uniforms
    u_hard_scan: gl::types::GLint,
    u_hard_pix: gl::types::GLint,
    u_warp_x: gl::types::GLint,
    u_warp_y: gl::types::GLint,
    u_mask_dark: gl::types::GLint,
    u_mask_light: gl::types::GLint,
    u_scale_in_linear_gamma: gl::types::GLint,
    u_shadow_mask: gl::types::GLint,
    u_bright_boost: gl::types::GLint,
    u_hard_bloom_scan: gl::types::GLint,
    u_hard_bloom_pix: gl::types::GLint,
    u_bloom_amount: gl::types::GLint,
    u_shape: gl::types::GLint,
}

impl ShaderProgram {
    fn new(vertex_source: &str, fragment_source: &str) -> Result<Self, String> {
        let program = create_shader_program(vertex_source, fragment_source)?;

        unsafe {
            Ok(ShaderProgram {
                program,
                a_position: gl::GetAttribLocation(program, b"a_position\0".as_ptr() as *const _),
                a_tex_coord: gl::GetAttribLocation(program, b"a_texCoord\0".as_ptr() as *const _),
                u_texture: gl::GetUniformLocation(program, b"u_texture\0".as_ptr() as *const _),
                u_texture_size: gl::GetUniformLocation(
                    program,
                    b"u_textureSize\0".as_ptr() as *const _,
                ),
                u_source_size: gl::GetUniformLocation(
                    program,
                    b"u_sourceSize\0".as_ptr() as *const _,
                ),
                u_output_size: gl::GetUniformLocation(
                    program,
                    b"u_outputSize\0".as_ptr() as *const _,
                ),
                u_hard_scan: gl::GetUniformLocation(program, b"u_hardScan\0".as_ptr() as *const _),
                u_hard_pix: gl::GetUniformLocation(program, b"u_hardPix\0".as_ptr() as *const _),
                u_warp_x: gl::GetUniformLocation(program, b"u_warpX\0".as_ptr() as *const _),
                u_warp_y: gl::GetUniformLocation(program, b"u_warpY\0".as_ptr() as *const _),
                u_mask_dark: gl::GetUniformLocation(program, b"u_maskDark\0".as_ptr() as *const _),
                u_mask_light: gl::GetUniformLocation(
                    program,
                    b"u_maskLight\0".as_ptr() as *const _,
                ),
                u_scale_in_linear_gamma: gl::GetUniformLocation(
                    program,
                    b"u_scaleInLinearGamma\0".as_ptr() as *const _,
                ),
                u_shadow_mask: gl::GetUniformLocation(
                    program,
                    b"u_shadowMask\0".as_ptr() as *const _,
                ),
                u_bright_boost: gl::GetUniformLocation(
                    program,
                    b"u_brightBoost\0".as_ptr() as *const _,
                ),
                u_hard_bloom_scan: gl::GetUniformLocation(
                    program,
                    b"u_hardBloomScan\0".as_ptr() as *const _,
                ),
                u_hard_bloom_pix: gl::GetUniformLocation(
                    program,
                    b"u_hardBloomPix\0".as_ptr() as *const _,
                ),
                u_bloom_amount: gl::GetUniformLocation(
                    program,
                    b"u_bloomAmount\0".as_ptr() as *const _,
                ),
                u_shape: gl::GetUniformLocation(program, b"u_shape\0".as_ptr() as *const _),
            })
        }
    }

    fn delete(&self) {
        unsafe {
            gl::DeleteProgram(self.program);
        }
    }
}

/// NTSC two-pass shader state
struct NtscShader {
    pass1_program: GLuint,
    pass2_program: GLuint,
    // Pass 1 attributes
    pass1_a_position: gl::types::GLint,
    pass1_a_tex_coord: gl::types::GLint,
    // Pass 1 uniforms
    pass1_u_texture: gl::types::GLint,
    pass1_u_output_size: gl::types::GLint,
    pass1_u_frame_count: gl::types::GLint,
    pass1_u_chroma_encode: gl::types::GLint,
    // Pass 2 attributes
    pass2_a_position: gl::types::GLint,
    pass2_a_tex_coord: gl::types::GLint,
    // Pass 2 uniforms
    pass2_u_texture: gl::types::GLint,
    pass2_u_source_size: gl::types::GLint,
    pass2_u_chroma_encode: gl::types::GLint,
    pass2_u_chroma_sum: gl::types::GLint,
    // Intermediate framebuffer and texture
    framebuffer: GLuint,
    intermediate_texture: GLuint,
    intermediate_width: u32,
    intermediate_height: u32,
}

impl NtscShader {
    fn new() -> Result<Self, String> {
        let pass1_program =
            create_shader_program(NTSC_PASS1_VERTEX_SHADER_SOURCE, NTSC_PASS1_FRAGMENT_SHADER_SOURCE)?;
        let pass2_program =
            create_shader_program(NTSC_PASS2_VERTEX_SHADER_SOURCE, NTSC_PASS2_FRAGMENT_SHADER_SOURCE)?;

        let mut framebuffer = 0;
        let mut intermediate_texture = 0;

        unsafe {
            // Create framebuffer for intermediate pass
            gl::GenFramebuffers(1, &mut framebuffer);
            gl::GenTextures(1, &mut intermediate_texture);

            let pass1_a_position =
                gl::GetAttribLocation(pass1_program, b"a_position\0".as_ptr() as *const _);
            let pass1_a_tex_coord =
                gl::GetAttribLocation(pass1_program, b"a_texCoord\0".as_ptr() as *const _);
            let pass1_u_texture =
                gl::GetUniformLocation(pass1_program, b"u_texture\0".as_ptr() as *const _);
            let pass1_u_output_size =
                gl::GetUniformLocation(pass1_program, b"u_outputSize\0".as_ptr() as *const _);
            let pass1_u_frame_count =
                gl::GetUniformLocation(pass1_program, b"u_frameCount\0".as_ptr() as *const _);
            let pass1_u_chroma_encode =
                gl::GetUniformLocation(pass1_program, b"u_chromaEncode\0".as_ptr() as *const _);

            let pass2_a_position =
                gl::GetAttribLocation(pass2_program, b"a_position\0".as_ptr() as *const _);
            let pass2_a_tex_coord =
                gl::GetAttribLocation(pass2_program, b"a_texCoord\0".as_ptr() as *const _);
            let pass2_u_texture =
                gl::GetUniformLocation(pass2_program, b"u_texture\0".as_ptr() as *const _);
            let pass2_u_source_size =
                gl::GetUniformLocation(pass2_program, b"u_sourceSize\0".as_ptr() as *const _);
            let pass2_u_chroma_encode =
                gl::GetUniformLocation(pass2_program, b"u_chromaEncode\0".as_ptr() as *const _);
            let pass2_u_chroma_sum =
                gl::GetUniformLocation(pass2_program, b"u_chromaSum\0".as_ptr() as *const _);

            Ok(NtscShader {
                pass1_program,
                pass2_program,
                pass1_a_position,
                pass1_a_tex_coord,
                pass1_u_texture,
                pass1_u_output_size,
                pass1_u_frame_count,
                pass1_u_chroma_encode,
                pass2_a_position,
                pass2_a_tex_coord,
                pass2_u_texture,
                pass2_u_source_size,
                pass2_u_chroma_encode,
                pass2_u_chroma_sum,
                framebuffer,
                intermediate_texture,
                intermediate_width: 0,
                intermediate_height: 0,
            })
        }
    }

    fn ensure_intermediate_texture(&mut self, width: u32, height: u32) -> Result<(), String> {
        if self.intermediate_width == width && self.intermediate_height == height {
            return Ok(());
        }

        unsafe {
            gl::BindTexture(gl::TEXTURE_2D, self.intermediate_texture);
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGB as i32,
                width as i32,
                height as i32,
                0,
                gl::RGB,
                gl::UNSIGNED_BYTE,
                ptr::null(),
            );
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);

            gl::BindFramebuffer(gl::FRAMEBUFFER, self.framebuffer);
            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                self.intermediate_texture,
                0,
            );

            let status = gl::CheckFramebufferStatus(gl::FRAMEBUFFER);
            if status != gl::FRAMEBUFFER_COMPLETE {
                return Err(format!("Framebuffer incomplete: {}", status));
            }

            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
        }

        self.intermediate_width = width;
        self.intermediate_height = height;
        Ok(())
    }

    fn delete(&self) {
        unsafe {
            gl::DeleteProgram(self.pass1_program);
            gl::DeleteProgram(self.pass2_program);
            gl::DeleteFramebuffers(1, &self.framebuffer);
            gl::DeleteTextures(1, &self.intermediate_texture);
        }
    }
}

pub struct ShaderManager {
    current_filter: FilterType,
    stock_shader: Option<ShaderProgram>,
    crt_shader: Option<ShaderProgram>,
    ntsc_shader: Option<NtscShader>,
    smooth_shader: Option<ShaderProgram>,
    crt_params: CrtParams,
    frame_count: usize,
    output_texture: Option<GLuint>,
    output_size: Option<(u32, u32)>,
    // Vertex buffers for quad rendering
    position_buffer: GLuint,
    tex_coord_buffer: GLuint,
}

impl ShaderManager {
    pub fn new() -> Self {
        let mut position_buffer = 0;
        let mut tex_coord_buffer = 0;

        unsafe {
            // Create vertex buffers for full-screen quad
            gl::GenBuffers(1, &mut position_buffer);
            gl::BindBuffer(gl::ARRAY_BUFFER, position_buffer);
            let positions: [f32; 8] = [-1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0];
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (positions.len() * std::mem::size_of::<f32>()) as isize,
                positions.as_ptr() as *const _,
                gl::STATIC_DRAW,
            );

            gl::GenBuffers(1, &mut tex_coord_buffer);
            gl::BindBuffer(gl::ARRAY_BUFFER, tex_coord_buffer);
            let tex_coords: [f32; 8] = [0.0, 1.0, 1.0, 1.0, 0.0, 0.0, 1.0, 0.0];
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (tex_coords.len() * std::mem::size_of::<f32>()) as isize,
                tex_coords.as_ptr() as *const _,
                gl::STATIC_DRAW,
            );
        }

        ShaderManager {
            current_filter: FilterType::Stock,
            stock_shader: None,
            crt_shader: None,
            ntsc_shader: None,
            smooth_shader: None,
            crt_params: CrtParams::default(),
            frame_count: 0,
            output_texture: None,
            output_size: None,
            position_buffer,
            tex_coord_buffer,
        }
    }

    fn ensure_shader_compiled(&mut self, filter: FilterType) -> Result<(), String> {
        match filter {
            FilterType::Stock => {
                if self.stock_shader.is_none() {
                    self.stock_shader = Some(ShaderProgram::new(
                        VERTEX_SHADER_SOURCE,
                        STOCK_FRAGMENT_SHADER_SOURCE,
                    )?);
                }
            }
            FilterType::Crt => {
                if self.crt_shader.is_none() {
                    self.crt_shader = Some(ShaderProgram::new(
                        VERTEX_SHADER_SOURCE,
                        CRT_FRAGMENT_SHADER_SOURCE,
                    )?);
                }
            }
            FilterType::Ntsc => {
                if self.ntsc_shader.is_none() {
                    self.ntsc_shader = Some(NtscShader::new()?);
                }
            }
            FilterType::Smooth => {
                if self.smooth_shader.is_none() {
                    self.smooth_shader = Some(ShaderProgram::new(
                        VERTEX_SHADER_SOURCE,
                        SMOOTH_FRAGMENT_SHADER_SOURCE,
                    )?);
                }
            }
        }
        Ok(())
    }

    pub fn load_preset(
        &mut self,
        preset_path: &std::path::Path,
        _gl_context: std::sync::Arc<glow::Context>,
    ) -> Result<(), String> {
        // Parse filter name from preset path
        let filename = preset_path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("Invalid preset path")?;

        let filter = if filename.contains("stock") {
            FilterType::Stock
        } else if filename.contains("crt") {
            FilterType::Crt
        } else if filename.contains("ntsc") {
            FilterType::Ntsc
        } else if filename.contains("xbrz") || filename.contains("smooth") {
            FilterType::Smooth
        } else {
            return Err(format!("Unknown filter type in preset: {}", filename));
        };

        self.ensure_shader_compiled(filter)?;
        self.current_filter = filter;
        self.frame_count = 0;

        Ok(())
    }

    fn ensure_output_texture(&mut self, width: u32, height: u32) -> Result<GLuint, String> {
        if width == 0 || height == 0 {
            return Err("Invalid output size".to_string());
        }

        let desired_size = (width, height);
        let needs_realloc = self.output_size != Some(desired_size) || self.output_texture.is_none();

        if needs_realloc {
            let tex = unsafe {
                let mut tex: GLuint = 0;
                gl::GenTextures(1, &mut tex);
                if tex == 0 {
                    return Err("Failed to create output texture".to_string());
                }

                gl::BindTexture(gl::TEXTURE_2D, tex);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
                gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);

                gl::TexImage2D(
                    gl::TEXTURE_2D,
                    0,
                    gl::RGB8 as i32,
                    width as i32,
                    height as i32,
                    0,
                    gl::RGB,
                    gl::UNSIGNED_BYTE,
                    ptr::null(),
                );

                tex
            };

            if let Some(old) = self.output_texture.take() {
                unsafe {
                    gl::DeleteTextures(1, &old);
                }
            }

            self.output_texture = Some(tex);
            self.output_size = Some(desired_size);
        }

        Ok(self.output_texture.expect("output texture must be set"))
    }

    fn bind_quad_attributes(&self, program: &ShaderProgram) {
        unsafe {
            if program.a_position != -1 {
                gl::BindBuffer(gl::ARRAY_BUFFER, self.position_buffer);
                gl::EnableVertexAttribArray(program.a_position as u32);
                gl::VertexAttribPointer(
                    program.a_position as u32,
                    2,
                    gl::FLOAT,
                    gl::FALSE,
                    0,
                    ptr::null(),
                );
            }
            if program.a_tex_coord != -1 {
                gl::BindBuffer(gl::ARRAY_BUFFER, self.tex_coord_buffer);
                gl::EnableVertexAttribArray(program.a_tex_coord as u32);
                gl::VertexAttribPointer(
                    program.a_tex_coord as u32,
                    2,
                    gl::FLOAT,
                    gl::FALSE,
                    0,
                    ptr::null(),
                );
            }
        }
    }

    fn render_single_pass(
        &self,
        program: &ShaderProgram,
        input_texture: GLuint,
        viewport_width: u32,
        viewport_height: u32,
        is_crt: bool,
    ) -> Result<(), String> {
        unsafe {
            gl::UseProgram(program.program);

            // Bind input texture
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, input_texture);
            if program.u_texture != -1 {
                gl::Uniform1i(program.u_texture, 0);
            }

            // Set uniforms
            if program.u_texture_size != -1 {
                gl::Uniform2f(program.u_texture_size, 256.0, 240.0);
            }
            if program.u_source_size != -1 {
                gl::Uniform2f(program.u_source_size, 256.0, 240.0);
            }
            if program.u_output_size != -1 {
                gl::Uniform2f(program.u_output_size, viewport_width as f32, viewport_height as f32);
            }

            // Set CRT-specific parameters
            if is_crt {
                if program.u_hard_scan != -1 {
                    gl::Uniform1f(program.u_hard_scan, self.crt_params.hard_scan);
                }
                if program.u_hard_pix != -1 {
                    gl::Uniform1f(program.u_hard_pix, self.crt_params.hard_pix);
                }
                if program.u_warp_x != -1 {
                    gl::Uniform1f(program.u_warp_x, self.crt_params.warp_x);
                }
                if program.u_warp_y != -1 {
                    gl::Uniform1f(program.u_warp_y, self.crt_params.warp_y);
                }
                if program.u_mask_dark != -1 {
                    gl::Uniform1f(program.u_mask_dark, self.crt_params.mask_dark);
                }
                if program.u_mask_light != -1 {
                    gl::Uniform1f(program.u_mask_light, self.crt_params.mask_light);
                }
                if program.u_scale_in_linear_gamma != -1 {
                    gl::Uniform1f(
                        program.u_scale_in_linear_gamma,
                        self.crt_params.scale_in_linear_gamma,
                    );
                }
                if program.u_shadow_mask != -1 {
                    gl::Uniform1f(program.u_shadow_mask, self.crt_params.shadow_mask);
                }
                if program.u_bright_boost != -1 {
                    gl::Uniform1f(program.u_bright_boost, self.crt_params.bright_boost);
                }
                if program.u_hard_bloom_scan != -1 {
                    gl::Uniform1f(program.u_hard_bloom_scan, self.crt_params.hard_bloom_scan);
                }
                if program.u_hard_bloom_pix != -1 {
                    gl::Uniform1f(program.u_hard_bloom_pix, self.crt_params.hard_bloom_pix);
                }
                if program.u_bloom_amount != -1 {
                    gl::Uniform1f(program.u_bloom_amount, self.crt_params.bloom_amount);
                }
                if program.u_shape != -1 {
                    gl::Uniform1f(program.u_shape, self.crt_params.shape);
                }
            }

            self.bind_quad_attributes(program);
            gl::DrawArrays(gl::TRIANGLE_STRIP, 0, 4);
        }

        Ok(())
    }

    fn render_ntsc_pass(
        &mut self,
        input_texture: GLuint,
        viewport_width: u32,
        viewport_height: u32,
    ) -> Result<(), String> {
        let ntsc = self.ntsc_shader.as_mut().ok_or("NTSC shader not loaded")?;

        // Intermediate texture is 4x wider than input
        let intermediate_width = 256 * 4;
        let intermediate_height = 240;
        ntsc.ensure_intermediate_texture(intermediate_width, intermediate_height)?;

        unsafe {
            // Pass 1: Encode to YIQ
            gl::BindFramebuffer(gl::FRAMEBUFFER, ntsc.framebuffer);
            gl::Viewport(0, 0, intermediate_width as i32, intermediate_height as i32);

            gl::UseProgram(ntsc.pass1_program);

            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, input_texture);
            if ntsc.pass1_u_texture != -1 {
                gl::Uniform1i(ntsc.pass1_u_texture, 0);
            }
            if ntsc.pass1_u_output_size != -1 {
                gl::Uniform2f(
                    ntsc.pass1_u_output_size,
                    intermediate_width as f32,
                    intermediate_height as f32,
                );
            }
            if ntsc.pass1_u_frame_count != -1 {
                gl::Uniform1f(ntsc.pass1_u_frame_count, (self.frame_count % 3) as f32);
            }
            if ntsc.pass1_u_chroma_encode != -1 {
                gl::Uniform1f(ntsc.pass1_u_chroma_encode, 0.0);
            }

            if ntsc.pass1_a_position != -1 {
                gl::BindBuffer(gl::ARRAY_BUFFER, self.position_buffer);
                gl::EnableVertexAttribArray(ntsc.pass1_a_position as u32);
                gl::VertexAttribPointer(
                    ntsc.pass1_a_position as u32,
                    2,
                    gl::FLOAT,
                    gl::FALSE,
                    0,
                    ptr::null(),
                );
            }
            if ntsc.pass1_a_tex_coord != -1 {
                gl::BindBuffer(gl::ARRAY_BUFFER, self.tex_coord_buffer);
                gl::EnableVertexAttribArray(ntsc.pass1_a_tex_coord as u32);
                gl::VertexAttribPointer(
                    ntsc.pass1_a_tex_coord as u32,
                    2,
                    gl::FLOAT,
                    gl::FALSE,
                    0,
                    ptr::null(),
                );
            }

            gl::DrawArrays(gl::TRIANGLE_STRIP, 0, 4);

            // Pass 2: Decode from YIQ to RGB
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::Viewport(0, 0, viewport_width as i32, viewport_height as i32);

            gl::UseProgram(ntsc.pass2_program);

            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, ntsc.intermediate_texture);
            if ntsc.pass2_u_texture != -1 {
                gl::Uniform1i(ntsc.pass2_u_texture, 0);
            }
            if ntsc.pass2_u_source_size != -1 {
                gl::Uniform2f(
                    ntsc.pass2_u_source_size,
                    intermediate_width as f32,
                    intermediate_height as f32,
                );
            }
            if ntsc.pass2_u_chroma_encode != -1 {
                gl::Uniform1f(ntsc.pass2_u_chroma_encode, 0.0);
            }
            if ntsc.pass2_u_chroma_sum != -1 {
                gl::Uniform1f(ntsc.pass2_u_chroma_sum, 0.538021759);
            }

            if ntsc.pass2_a_position != -1 {
                gl::BindBuffer(gl::ARRAY_BUFFER, self.position_buffer);
                gl::EnableVertexAttribArray(ntsc.pass2_a_position as u32);
                gl::VertexAttribPointer(
                    ntsc.pass2_a_position as u32,
                    2,
                    gl::FLOAT,
                    gl::FALSE,
                    0,
                    ptr::null(),
                );
            }
            if ntsc.pass2_a_tex_coord != -1 {
                gl::BindBuffer(gl::ARRAY_BUFFER, self.tex_coord_buffer);
                gl::EnableVertexAttribArray(ntsc.pass2_a_tex_coord as u32);
                gl::VertexAttribPointer(
                    ntsc.pass2_a_tex_coord as u32,
                    2,
                    gl::FLOAT,
                    gl::FALSE,
                    0,
                    ptr::null(),
                );
            }

            gl::DrawArrays(gl::TRIANGLE_STRIP, 0, 4);
        }

        Ok(())
    }

    pub fn apply_shader(
        &mut self,
        input_texture: GLuint,
        viewport_width: u32,
        viewport_height: u32,
    ) -> Result<(), String> {
        self.ensure_shader_compiled(self.current_filter)?;

        // For NTSC, render directly to default framebuffer
        if self.current_filter == FilterType::Ntsc {
            self.render_ntsc_pass(input_texture, viewport_width, viewport_height)?;
            self.frame_count = self.frame_count.wrapping_add(1);
            return Ok(());
        }

        // For other filters, render to output texture
        let output_texture = self.ensure_output_texture(viewport_width, viewport_height)?;

        // Create a framebuffer to render to the output texture
        let mut fbo = 0;
        unsafe {
            gl::GenFramebuffers(1, &mut fbo);
            gl::BindFramebuffer(gl::FRAMEBUFFER, fbo);
            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                output_texture,
                0,
            );

            let status = gl::CheckFramebufferStatus(gl::FRAMEBUFFER);
            if status != gl::FRAMEBUFFER_COMPLETE {
                gl::DeleteFramebuffers(1, &fbo);
                return Err(format!("Framebuffer incomplete: {}", status));
            }

            gl::Viewport(0, 0, viewport_width as i32, viewport_height as i32);
        }

        match self.current_filter {
            FilterType::Stock => {
                let shader = self.stock_shader.as_ref().ok_or("Stock shader not loaded")?;
                self.render_single_pass(shader, input_texture, viewport_width, viewport_height, false)?;
            }
            FilterType::Crt => {
                let shader = self.crt_shader.as_ref().ok_or("CRT shader not loaded")?;
                self.render_single_pass(shader, input_texture, viewport_width, viewport_height, true)?;
            }
            FilterType::Smooth => {
                let shader = self.smooth_shader.as_ref().ok_or("Smooth shader not loaded")?;
                // For smooth filter, use LINEAR filtering on the input texture
                unsafe {
                    gl::BindTexture(gl::TEXTURE_2D, input_texture);
                    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR_MIPMAP_LINEAR as i32);
                    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
                }
                self.render_single_pass(shader, input_texture, viewport_width, viewport_height, false)?;
            }
            FilterType::Ntsc => unreachable!(), // Handled above
        }

        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::DeleteFramebuffers(1, &fbo);
        }

        self.frame_count = self.frame_count.wrapping_add(1);
        Ok(())
    }

    pub fn output_texture(&self) -> Option<GLuint> {
        if self.current_filter == FilterType::Ntsc {
            None // NTSC renders directly to framebuffer
        } else {
            self.output_texture
        }
    }

    pub fn cycle_shader(&mut self, _gl_context: std::sync::Arc<glow::Context>) -> Result<(), String> {
        let current_index = FILTER_ORDER
            .iter()
            .position(|&f| f == self.current_filter)
            .unwrap_or(0);
        let next_index = (current_index + 1) % FILTER_ORDER.len();
        self.current_filter = FILTER_ORDER[next_index];
        self.ensure_shader_compiled(self.current_filter)?;
        self.frame_count = 0;
        Ok(())
    }

    pub fn current_preset_name(&self) -> Option<&str> {
        Some(self.current_filter.name())
    }

    pub fn has_shader(&self) -> bool {
        match self.current_filter {
            FilterType::Stock => self.stock_shader.is_some(),
            FilterType::Crt => self.crt_shader.is_some(),
            FilterType::Ntsc => self.ntsc_shader.is_some(),
            FilterType::Smooth => self.smooth_shader.is_some(),
        }
    }
}

impl Drop for ShaderManager {
    fn drop(&mut self) {
        if let Some(tex) = self.output_texture.take() {
            unsafe {
                gl::DeleteTextures(1, &tex);
            }
        }

        if let Some(shader) = self.stock_shader.take() {
            shader.delete();
        }
        if let Some(shader) = self.crt_shader.take() {
            shader.delete();
        }
        if let Some(shader) = self.ntsc_shader.take() {
            shader.delete();
        }
        if let Some(shader) = self.smooth_shader.take() {
            shader.delete();
        }

        unsafe {
            gl::DeleteBuffers(1, &self.position_buffer);
            gl::DeleteBuffers(1, &self.tex_coord_buffer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_type_name() {
        assert_eq!(FilterType::Stock.name(), "None");
        assert_eq!(FilterType::Crt.name(), "CRT");
        assert_eq!(FilterType::Ntsc.name(), "NTSC");
        assert_eq!(FilterType::Smooth.name(), "Smooth");
    }

    #[test]
    fn test_filter_type_from_name() {
        assert_eq!(FilterType::from_name("stock"), Some(FilterType::Stock));
        assert_eq!(FilterType::from_name("none"), Some(FilterType::Stock));
        assert_eq!(FilterType::from_name("crt"), Some(FilterType::Crt));
        assert_eq!(FilterType::from_name("ntsc"), Some(FilterType::Ntsc));
        assert_eq!(FilterType::from_name("smooth"), Some(FilterType::Smooth));
        assert_eq!(FilterType::from_name("unknown"), None);
    }

    #[test]
    fn test_crt_params_default() {
        let params = CrtParams::default();
        assert_eq!(params.hard_scan, -8.0);
        assert_eq!(params.shadow_mask, 3.0);
        assert_eq!(params.bloom_amount, 0.15);
    }
}
