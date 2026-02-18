use super::sdl_render_target::SdlRenderTarget;
use crate::app_context::AppContext;
use crate::console::Config;
use crate::debugging::breakpoints::BreakpointList;
use crate::debugging::ui::DebuggerUiAction;
use crate::rendering::input::{InputEvent, MouseButton as RenderMouseButton};
use crate::rendering::{GlBackend, ProcAddressLoader};
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::mouse::MouseButton;
use sdl2::video::{FullscreenType, GLProfile, Window, WindowPos};
use std::rc::Rc;

/// SDL-specific wrapper that owns a `GlBackend` and SDL window/context.
///
/// This module translates SDL events into renderer input events and
/// configures the GL context/window settings based on emulator config.
pub struct SdlGlWrapper {
    gl_backend: GlBackend,
}

impl SdlGlWrapper {
    /// Creates a new SDL-backed renderer instance.
    pub fn new(
        sdl_context: &sdl2::Sdl,
        config: &Config,
        app_context: AppContext,
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

        let target_display = select_target_display(&video_subsystem, config)?;
        let (window_width, window_height) =
            resolve_window_size(&video_subsystem, config, target_display)?;

        let mut window_builder =
            video_subsystem.window("NES Emulator in Rust", window_width, window_height);
        window_builder.opengl();

        window_builder.position_centered();
        if !config.fullscreen {
            window_builder.resizable();
        }

        let mut window = window_builder.build().map_err(|e| e.to_string())?;

        if let Some(display) = target_display {
            center_fullscreen_window(
                &video_subsystem,
                &mut window,
                display,
                window_width,
                window_height,
            );
            window
                .set_fullscreen(FullscreenType::Desktop)
                .map_err(|e| e.to_string())?;
        }

        let gl_context = window.gl_create_context().map_err(|e| e.to_string())?;
        window
            .gl_make_current(&gl_context)
            .map_err(|e| e.to_string())?;

        video_subsystem
            .gl_set_swap_interval(if config.vsync_enabled { 1 } else { 0 })
            .map_err(|e| e.to_string())?;

        gl::load_with(|s| video_subsystem.gl_get_proc_address(s) as _);

        let proc_address: ProcAddressLoader =
            Rc::new(move |s| video_subsystem.gl_get_proc_address(s) as *const _);

        let render_target = Box::new(SdlRenderTarget { window, gl_context });
        let mut gl_backend = GlBackend::new(
            render_target,
            proc_address,
            config.shader_path.as_deref(),
            app_context,
        )?;
        gl_backend.set_debugger_alpha(config.debugger_alpha);

        Ok(Self { gl_backend })
    }

    /// Handles an SDL event and forwards input to the renderer.
    pub fn handle_event(&mut self, event: &Event) {
        if let Some(input_event) = translate_event(event) {
            self.gl_backend.handle_input(&input_event);
        }
    }

    /// Returns the current SDL window size as `(width, height)` in pixels.
    pub fn window_size(&self) -> (u32, u32) {
        self.gl_backend.window_size()
    }

    /// Renders a frame and optional debugger overlay.
    pub fn render(
        &mut self,
        nes: &crate::console::Nes,
        show_debugger: bool,
        overlay_text: Option<&str>,
        overlay_blink_red: bool,
        crosshair: Option<crate::rendering::Crosshair>,
    ) -> DebuggerUiAction {
        self.gl_backend.render(
            nes,
            show_debugger,
            overlay_text,
            overlay_blink_red,
            crosshair,
        )
    }

    /// Cycles through available shader presets.
    pub fn cycle_shader(&mut self) {
        self.gl_backend.cycle_shader();
    }

    /// Updates the breakpoint list used by the debugger UI.
    pub fn update_breakpoints(&mut self, breakpoints: &BreakpointList) {
        self.gl_backend.update_breakpoints(breakpoints);
    }

    /// Enables or disables fullscreen mode for the SDL window.
    pub fn set_fullscreen(&mut self, enabled: bool) -> Result<(), String> {
        self.gl_backend.set_fullscreen(enabled)
    }
}

/// Chooses which display to use for fullscreen rendering.
fn select_target_display(
    video_subsystem: &sdl2::VideoSubsystem,
    config: &Config,
) -> Result<Option<i32>, String> {
    if !config.fullscreen {
        return Ok(None);
    }

    let display_count = video_subsystem.num_video_displays().unwrap_or(1);
    let target_display = match config.fullscreen_display {
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

    Ok(Some(target_display))
}

/// Resolves window size based on fullscreen state and aspect policy.
fn resolve_window_size(
    video_subsystem: &sdl2::VideoSubsystem,
    config: &Config,
    target_display: Option<i32>,
) -> Result<(u32, u32), String> {
    if let Some(display) = target_display {
        match video_subsystem.display_bounds(display) {
            Ok(bounds) => Ok((bounds.width(), bounds.height())),
            Err(e) => Err(format!(
                "Failed to query bounds for display {display}: {e}. Cannot enter fullscreen mode."
            )),
        }
    } else {
        Ok(GlBackend::windowed_dimensions(config.window_height))
    }
}

/// Centers the SDL window within the selected display bounds.
fn center_fullscreen_window(
    video_subsystem: &sdl2::VideoSubsystem,
    window: &mut Window,
    display: i32,
    window_width: u32,
    window_height: u32,
) {
    if let Ok(bounds) = video_subsystem.display_bounds(display) {
        let x = bounds.x() + (bounds.width() as i32 - window_width as i32) / 2;
        let y = bounds.y() + (bounds.height() as i32 - window_height as i32) / 2;
        window.set_position(WindowPos::Positioned(x), WindowPos::Positioned(y));
    }
}

/// Converts SDL input events into renderer input events.
fn translate_event(event: &Event) -> Option<InputEvent> {
    match event {
        Event::MouseMotion { x, y, .. } => Some(InputEvent::MouseMotion {
            x: *x as f32,
            y: *y as f32,
        }),
        Event::MouseButtonDown { mouse_btn, .. } => Some(InputEvent::MouseButton {
            button: map_mouse_button(*mouse_btn)?,
            pressed: true,
        }),
        Event::MouseButtonUp { mouse_btn, .. } => Some(InputEvent::MouseButton {
            button: map_mouse_button(*mouse_btn)?,
            pressed: false,
        }),
        Event::MouseWheel { x, y, .. } => Some(InputEvent::MouseWheel {
            x: *x as f32,
            y: *y as f32,
        }),
        Event::TextInput { text, .. } => Some(InputEvent::TextInput(text.clone())),
        Event::KeyDown {
            keycode, repeat, ..
        } => translate_key_input(*keycode, *repeat, true),
        Event::KeyUp { keycode, .. } => translate_key_input(*keycode, false, false),
        _ => None,
    }
}

fn translate_key_input(keycode: Option<Keycode>, repeat: bool, down: bool) -> Option<InputEvent> {
    if repeat {
        return None;
    }

    let keycode = keycode?;
    map_key(keycode).map(|key| InputEvent::Key { key, down })
}

/// Maps SDL mouse button identifiers to renderer mouse buttons.
fn map_mouse_button(button: MouseButton) -> Option<RenderMouseButton> {
    match button {
        MouseButton::Left => Some(RenderMouseButton::Left),
        MouseButton::Right => Some(RenderMouseButton::Right),
        MouseButton::Middle => Some(RenderMouseButton::Middle),
        _ => None,
    }
}

/// Maps SDL keycodes into ImGui keys used by the renderer.
fn map_key(keycode: Keycode) -> Option<imgui::Key> {
    // Minimal key mapping needed for common interactions.
    match keycode {
        Keycode::Tab => Some(imgui::Key::Tab),
        Keycode::Left => Some(imgui::Key::LeftArrow),
        Keycode::Right => Some(imgui::Key::RightArrow),
        Keycode::Up => Some(imgui::Key::UpArrow),
        Keycode::Down => Some(imgui::Key::DownArrow),
        Keycode::PageUp => Some(imgui::Key::PageUp),
        Keycode::PageDown => Some(imgui::Key::PageDown),
        Keycode::Home => Some(imgui::Key::Home),
        Keycode::End => Some(imgui::Key::End),
        Keycode::Insert => Some(imgui::Key::Insert),
        Keycode::Delete => Some(imgui::Key::Delete),
        Keycode::Backspace => Some(imgui::Key::Backspace),
        Keycode::Space => Some(imgui::Key::Space),
        Keycode::Return => Some(imgui::Key::Enter),
        Keycode::Escape => Some(imgui::Key::Escape),
        Keycode::A => Some(imgui::Key::A),
        Keycode::C => Some(imgui::Key::C),
        Keycode::V => Some(imgui::Key::V),
        Keycode::X => Some(imgui::Key::X),
        Keycode::Y => Some(imgui::Key::Y),
        Keycode::Z => Some(imgui::Key::Z),
        Keycode::F1 => Some(imgui::Key::F1),
        _ => None,
    }
}
