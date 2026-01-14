use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::mouse::MouseButton;
use sdl2::video::{FullscreenType, GLContext, GLProfile, Window, WindowPos};

use crate::debugger;
use crate::debugger::ui as debugger_ui;
use crate::nes::TvSystem;
use crate::shader_manager::ShaderManager;
use std::time::Instant;

pub(crate) struct GlBackend {
    window: Window,
    gl_context: GLContext,
    glow_context: std::sync::Arc<glow::Context>,
    imgui: imgui::Context,
    renderer: imgui_opengl_renderer::Renderer,
    nes_texture: gl::types::GLuint,
    nes_texture_id: imgui::TextureId,
    framebuffer: Vec<u8>,
    last_frame: Instant,
    debugger_view_state: debugger::DebuggerViewState,
    shader_manager: ShaderManager,
}

impl GlBackend {
    pub(crate) fn new(
        sdl_context: &sdl2::Sdl,
        tv_system: TvSystem,
        scale: f32,
        vsync_enabled: bool,
        fullscreen: bool,
        fullscreen_display: Option<i32>,
        shader_path: Option<&str>,
    ) -> Result<Self, String> {
        let video_subsystem = sdl_context.video()?;

        {
            let gl_attr = video_subsystem.gl_attr();
            gl_attr.set_context_profile(GLProfile::Core);
            // macOS core profile requires 3.2+ for forward-compatible contexts.
            gl_attr.set_context_version(3, 2);
            gl_attr.set_double_buffer(true);
            gl_attr.set_depth_size(0);
            gl_attr.set_stencil_size(0);
        }

        let base_width = tv_system.screen_width();
        let base_height = tv_system.screen_height();
        let scaled_width = (base_width as f32 * scale) as u32;
        let scaled_height = (base_height as f32 * scale) as u32;

        let mut window_builder =
            video_subsystem.window("NES Emulator in Rust", scaled_width, scaled_height);
        window_builder.opengl();

        window_builder.position_centered();
        if !fullscreen {
            window_builder.resizable();
        }

        let mut window = window_builder.build().map_err(|e| e.to_string())?;

        if fullscreen {
            let display_count = video_subsystem.num_video_displays().unwrap_or(1);
            let target_display = match fullscreen_display {
                Some(display) => display,
                None => {
                    if display_count >= 2 {
                        1
                    } else {
                        0
                    }
                }
            };

            if target_display < 0 || target_display >= display_count {
                return Err(format!(
                    "Invalid --display {target_display}. Available displays: 0..{}",
                    display_count.saturating_sub(1)
                ));
            }

            if let Ok(bounds) = video_subsystem.display_bounds(target_display) {
                let x = bounds.x() + (bounds.width() as i32 - scaled_width as i32) / 2;
                let y = bounds.y() + (bounds.height() as i32 - scaled_height as i32) / 2;
                window.set_position(WindowPos::Positioned(x), WindowPos::Positioned(y));
            }

            window
                .set_fullscreen(FullscreenType::Desktop)
                .map_err(|e| e.to_string())?;
        }

        let gl_context = window.gl_create_context().map_err(|e| e.to_string())?;
        window
            .gl_make_current(&gl_context)
            .map_err(|e| e.to_string())?;

        video_subsystem
            .gl_set_swap_interval(if vsync_enabled { 1 } else { 0 })
            .map_err(|e| e.to_string())?;

        gl::load_with(|s| video_subsystem.gl_get_proc_address(s) as _);

        unsafe {
            gl::Disable(gl::DEPTH_TEST);
            gl::Disable(gl::CULL_FACE);
            let (drawable_w, drawable_h) = window.drawable_size();
            gl::Viewport(0, 0, drawable_w as i32, drawable_h as i32);
            gl::ClearColor(0.0, 0.0, 0.0, 1.0);
        }

        let mut imgui = imgui::Context::create();
        imgui.set_ini_filename(None);

        let renderer = imgui_opengl_renderer::Renderer::new(&mut imgui, |s| {
            video_subsystem.gl_get_proc_address(s) as _
        });

        let (nes_texture, nes_texture_id) = unsafe {
            let mut tex: gl::types::GLuint = 0;
            gl::GenTextures(1, &mut tex);
            gl::BindTexture(gl::TEXTURE_2D, tex);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);

            // Allocate texture storage.
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGB8 as i32,
                256,
                240,
                0,
                gl::RGB,
                gl::UNSIGNED_BYTE,
                std::ptr::null(),
            );

            let id: imgui::TextureId = (tex as usize).into();
            (tex, id)
        };

        // Create glow context for librashader
        let glow_context = unsafe {
            std::sync::Arc::new(glow::Context::from_loader_function(|s| {
                video_subsystem.gl_get_proc_address(s) as *const _
            }))
        };

        let mut shader_manager = ShaderManager::new();

        // Load shader preset if specified
        if let Some(path) = shader_path {
            if let Err(e) =
                shader_manager.load_preset(std::path::Path::new(path), glow_context.clone())
            {
                eprintln!("Warning: Failed to load shader preset '{}': {}", path, e);
            }
        }

        Ok(Self {
            window,
            gl_context,
            glow_context,
            imgui,
            renderer,
            nes_texture,
            nes_texture_id,
            framebuffer: vec![0u8; 256 * 240 * 3],
            last_frame: Instant::now(),
            debugger_view_state: debugger::DebuggerViewState::default(),
            shader_manager,
        })
    }

    pub(crate) fn handle_event(&mut self, event: &Event) {
        let io = self.imgui.io_mut();

        match *event {
            Event::MouseMotion { x, y, .. } => {
                io.mouse_pos = [x as f32, y as f32];
            }
            Event::MouseButtonDown { mouse_btn, .. } => match mouse_btn {
                MouseButton::Left => io.mouse_down[0] = true,
                MouseButton::Right => io.mouse_down[1] = true,
                MouseButton::Middle => io.mouse_down[2] = true,
                _ => {}
            },
            Event::MouseButtonUp { mouse_btn, .. } => match mouse_btn {
                MouseButton::Left => io.mouse_down[0] = false,
                MouseButton::Right => io.mouse_down[1] = false,
                MouseButton::Middle => io.mouse_down[2] = false,
                _ => {}
            },
            Event::MouseWheel { x, y, .. } => {
                io.mouse_wheel_h += x as f32;
                io.mouse_wheel += y as f32;
            }
            Event::TextInput { ref text, .. } => {
                for ch in text.chars() {
                    io.add_input_character(ch);
                }
            }
            Event::KeyDown {
                keycode: Some(keycode),
                repeat: false,
                ..
            } => {
                Self::map_key(io, keycode, true);
            }
            Event::KeyUp {
                keycode: Some(keycode),
                ..
            } => {
                Self::map_key(io, keycode, false);
            }
            _ => {}
        }
    }

    fn map_key(io: &mut imgui::Io, keycode: Keycode, down: bool) {
        use imgui::Key;

        // Minimal key mapping needed for common interactions.
        let key = match keycode {
            Keycode::Tab => Some(Key::Tab),
            Keycode::Left => Some(Key::LeftArrow),
            Keycode::Right => Some(Key::RightArrow),
            Keycode::Up => Some(Key::UpArrow),
            Keycode::Down => Some(Key::DownArrow),
            Keycode::PageUp => Some(Key::PageUp),
            Keycode::PageDown => Some(Key::PageDown),
            Keycode::Home => Some(Key::Home),
            Keycode::End => Some(Key::End),
            Keycode::Insert => Some(Key::Insert),
            Keycode::Delete => Some(Key::Delete),
            Keycode::Backspace => Some(Key::Backspace),
            Keycode::Space => Some(Key::Space),
            Keycode::Return => Some(Key::Enter),
            Keycode::Escape => Some(Key::Escape),
            Keycode::A => Some(Key::A),
            Keycode::C => Some(Key::C),
            Keycode::V => Some(Key::V),
            Keycode::X => Some(Key::X),
            Keycode::Y => Some(Key::Y),
            Keycode::Z => Some(Key::Z),
            _ => None,
        };

        if let Some(key) = key {
            io.add_key_event(key, down);
        }
    }

    pub(crate) fn render(
        &mut self,
        nes: &crate::nes::Nes,
        show_debugger: bool,
    ) -> crate::debugger::ui::DebuggerUiAction {
        let mut action = crate::debugger::ui::DebuggerUiAction::default();

        // Per-frame IO updates
        let now = Instant::now();
        let dt = now.saturating_duration_since(self.last_frame);
        self.last_frame = now;
        self.imgui.io_mut().delta_time = dt.as_secs_f32().max(1.0 / 1000.0);

        let (win_w, win_h) = self.window.size();
        let (drawable_w, drawable_h) = self.window.drawable_size();

        // Keep the GL viewport in sync with the current drawable size.
        unsafe {
            gl::Viewport(0, 0, drawable_w as i32, drawable_h as i32);
        }

        let scale_x = if win_w == 0 {
            1.0
        } else {
            drawable_w as f32 / win_w as f32
        };
        let scale_y = if win_h == 0 {
            1.0
        } else {
            drawable_h as f32 / win_h as f32
        };

        {
            let io = self.imgui.io_mut();
            io.display_size = [win_w as f32, win_h as f32];
            io.display_framebuffer_scale = [scale_x, scale_y];
        }

        // Update NES texture (keep the PPU borrow short-lived so we can snapshot later).
        {
            let screen_buffer = nes.get_screen_buffer();
            screen_buffer.copy_buffer(&mut self.framebuffer);
        }

        unsafe {
            gl::BindTexture(gl::TEXTURE_2D, self.nes_texture);
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
            gl::TexSubImage2D(
                gl::TEXTURE_2D,
                0,
                0,
                0,
                256,
                240,
                gl::RGB,
                gl::UNSIGNED_BYTE,
                self.framebuffer.as_ptr() as *const _,
            );
            // librashader's OpenGL runtime uses sampler objects whose MIN_FILTER always
            // includes a mipmap variant (e.g. LINEAR_MIPMAP_LINEAR). Ensure the NES texture
            // is mipmap-complete, otherwise some drivers (notably macOS) will treat it as
            // unloadable and sample black.
            gl::GenerateMipmap(gl::TEXTURE_2D);
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }

        // Apply shader post-processing if a shader is loaded
        // The shader will render the NES texture to the screen with filtering applied
        // Note: librashader's OpenGL runtime renders into a texture-backed output, not
        // directly into the default framebuffer. We therefore render into an output texture
        // and draw that texture as the background.
        let mut shader_output_texture_id: Option<imgui::TextureId> = None;
        if self.shader_manager.has_shader() {
            // Compute a drawable-space letterbox size for the shader output.
            let drawable_w_f = drawable_w as f32;
            let drawable_h_f = drawable_h as f32;
            let nes_aspect = 256.0 / 240.0;
            let drawable_aspect = if drawable_h_f == 0.0 {
                nes_aspect
            } else {
                drawable_w_f / drawable_h_f
            };

            let (shader_out_w, shader_out_h) = if drawable_aspect > nes_aspect {
                (
                    ((drawable_h_f * nes_aspect) as u32).max(1),
                    drawable_h.max(1),
                )
            } else {
                (
                    drawable_w.max(1),
                    ((drawable_w_f / nes_aspect) as u32).max(1),
                )
            };

            if let Err(e) =
                self.shader_manager
                    .apply_shader(self.nes_texture, shader_out_w, shader_out_h)
            {
                eprintln!("Shader application error: {}", e);
            } else if let Some(tex) = self.shader_manager.output_texture() {
                shader_output_texture_id = Some((tex as usize).into());
            }
        }

        // Start ImGui frame
        {
            let ui = self.imgui.frame();

            // Draw NES frame as a background image, preserving aspect ratio with letterboxing.
            let win_w = win_w as f32;
            let win_h = win_h as f32;
            let nes_aspect = 256.0 / 240.0;
            let win_aspect = if win_h == 0.0 {
                nes_aspect
            } else {
                win_w / win_h
            };

            let (draw_w, draw_h) = if win_aspect > nes_aspect {
                // Window is wider than NES: fit height.
                (win_h * nes_aspect, win_h)
            } else {
                // Window is taller than NES: fit width.
                (win_w, win_w / nes_aspect)
            };

            let x0 = (win_w - draw_w) * 0.5;
            let y0 = (win_h - draw_h) * 0.5;

            // Only draw the NES texture as a background if no shader is active.
            // When a shader is active, we draw the shader output texture.
            //
            // Note: imgui 0.11 may produce draw_data with CmdLists=null when nothing is drawn.
            // imgui-opengl-renderer iterates draw_lists() unconditionally, which will panic
            // on a null pointer even when the count is 0. Ensure we always emit at least one
            // (invisible) draw command so the draw list pointer is non-null.
            if let Some(shader_tex_id) = shader_output_texture_id {
                ui.get_background_draw_list()
                    .add_image(shader_tex_id, [x0, y0], [x0 + draw_w, y0 + draw_h])
                    .build();
            } else {
                ui.get_background_draw_list()
                    .add_image(self.nes_texture_id, [x0, y0], [x0 + draw_w, y0 + draw_h])
                    .build();
            }

            if show_debugger {
                let snapshot = self.debugger_view_state.snapshot(nes);
                action = debugger_ui::render(&ui, &snapshot);
            }
        }

        self.renderer.render(&mut self.imgui);

        self.window.gl_swap_window();

        action
    }

    pub(crate) fn cycle_shader(&mut self) {
        if let Err(e) = self.shader_manager.cycle_shader(self.glow_context.clone()) {
            eprintln!("Error cycling shader: {}", e);
        } else if let Some(name) = self.shader_manager.current_preset_name() {
            println!("Switched to shader: {}", name);
        }
    }
}

impl Drop for GlBackend {
    fn drop(&mut self) {
        // Best-effort: make current and delete texture.
        let _ = self.window.gl_make_current(&self.gl_context);
        unsafe {
            gl::DeleteTextures(1, &self.nes_texture);
        }
    }
}
