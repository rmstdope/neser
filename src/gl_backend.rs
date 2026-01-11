use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::mouse::MouseButton;
use sdl2::video::{GLContext, GLProfile, Window};

use crate::debugger;
use crate::debugger_ui;
use crate::nes::TvSystem;
use std::time::Instant;

pub(crate) struct GlBackend {
    window: Window,
    gl_context: GLContext,
    imgui: imgui::Context,
    renderer: imgui_opengl_renderer::Renderer,
    nes_texture: gl::types::GLuint,
    nes_texture_id: imgui::TextureId,
    framebuffer: Vec<u8>,
    last_frame: Instant,
}

impl GlBackend {
    pub(crate) fn new(
        sdl_context: &sdl2::Sdl,
        tv_system: TvSystem,
        scale: f32,
        vsync_enabled: bool,
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

        let window = video_subsystem
            .window("NES Emulator in Rust", scaled_width, scaled_height)
            .position_centered()
            .opengl()
            .resizable()
            .build()
            .map_err(|e| e.to_string())?;

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
            gl::Viewport(0, 0, scaled_width as i32, scaled_height as i32);
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

        Ok(Self {
            window,
            gl_context,
            imgui,
            renderer,
            nes_texture,
            nes_texture_id,
            framebuffer: vec![0u8; 256 * 240 * 3],
            last_frame: Instant::now(),
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
    ) -> crate::debugger_ui::DebuggerUiAction {
        let mut action = crate::debugger_ui::DebuggerUiAction::default();

        // Per-frame IO updates
        let now = Instant::now();
        let dt = now.saturating_duration_since(self.last_frame);
        self.last_frame = now;
        self.imgui.io_mut().delta_time = dt.as_secs_f32().max(1.0 / 1000.0);

        let (win_w, win_h) = self.window.size();
        let (drawable_w, drawable_h) = self.window.drawable_size();
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
            let mut screen_buffer = nes.get_screen_buffer();
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
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }

        // Start ImGui frame
        {
            let ui = self.imgui.frame();

            // Draw NES frame as a background image (stretch to window).
            ui.get_background_draw_list()
                .add_image(
                    self.nes_texture_id,
                    [0.0, 0.0],
                    [win_w as f32, win_h as f32],
                )
                .build();

            if show_debugger {
                let snapshot = debugger::snapshot(nes);
                action = debugger_ui::render(&ui, &snapshot);
            }
        }

        self.renderer.render(&mut self.imgui);

        self.window.gl_swap_window();

        action
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
