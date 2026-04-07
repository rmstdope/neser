#![allow(dead_code)] // Public API for future use in native frontend
use super::render_target::WinitRenderTarget;
use crate::app_context::SharedAppContext;
use crate::debugging::breakpoints::BreakpointList;
use crate::debugging::ui::DebuggerUiAction;
use crate::rendering::input::{InputEvent, MouseButton as RenderMouseButton};
use crate::rendering::{Crosshair, GlBackend, ProcAddressLoader};

use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextApi, ContextAttributesBuilder, Version};
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin::surface::SurfaceAttributesBuilder;
use glutin_winit::DisplayBuilder;
use raw_window_handle::HasWindowHandle;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes};

use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::Arc;

/// Winit/glutin wrapper that owns a [`GlBackend`] and the native window.
///
/// Mirrors the role of `SdlGlWrapper`: sets up the OpenGL context via glutin,
/// creates a [`WinitRenderTarget`], and translates winit events into the
/// backend-agnostic [`InputEvent`]s consumed by the renderer.
pub struct NativeGlWrapper {
    gl_backend: GlBackend,
    render_target_window: Arc<Window>,
}

impl NativeGlWrapper {
    /// Creates a new winit/glutin-backed renderer for the given event loop.
    pub fn new(
        event_loop: &winit::event_loop::ActiveEventLoop,
        app_context: SharedAppContext,
    ) -> Result<Self, String> {
        let (
            fullscreen,
            vsync_enabled,
            shader_path,
            debugger_alpha,
            fullscreen_display,
            window_width,
            window_height,
        ) = {
            let ctx = app_context.borrow();
            let config = ctx.config();
            let (window_width, window_height) = GlBackend::windowed_dimensions(
                config.window_height,
                config.horizontal_overscan as u32,
                config.vertical_overscan as u32,
            );
            (
                config.fullscreen,
                config.vsync_enabled,
                config.shader_path.clone(),
                config.debugger_alpha,
                config.fullscreen_display,
                window_width,
                window_height,
            )
        };

        let monitors: Vec<_> = event_loop.available_monitors().collect();
        let target_display = select_target_display(fullscreen, fullscreen_display, monitors.len())?;

        // Build the window
        let mut window_attrs = WindowAttributes::default()
            .with_title("NES Emulator in Rust")
            .with_inner_size(LogicalSize::new(window_width, window_height))
            .with_resizable(!fullscreen);

        if let Some(display_index) = target_display {
            let monitor = monitors.into_iter().nth(display_index);
            window_attrs =
                window_attrs.with_fullscreen(Some(winit::window::Fullscreen::Borderless(monitor)));
        }

        // Configure glutin
        let config_template = ConfigTemplateBuilder::new()
            .with_alpha_size(8)
            .with_depth_size(0)
            .with_stencil_size(0);

        let display_builder = DisplayBuilder::new().with_window_attributes(Some(window_attrs));

        let (window, gl_config) = display_builder
            .build(event_loop, config_template, |configs| {
                configs
                    .reduce(|accum, config| {
                        if config.num_samples() > accum.num_samples() {
                            config
                        } else {
                            accum
                        }
                    })
                    .expect("no compatible GL configurations found")
            })
            .map_err(|e| format!("failed to build display: {e}"))?;

        let window = Arc::new(window.ok_or("failed to create window")?);
        let raw_window_handle = window
            .window_handle()
            .map_err(|e| format!("failed to get window handle: {e}"))?
            .as_raw();

        let gl_display = gl_config.display();

        // Create GL context (request OpenGL 3.2 Core for macOS compatibility)
        let context_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 2))))
            .build(Some(raw_window_handle));

        let not_current_context = unsafe {
            gl_display
                .create_context(&gl_config, &context_attributes)
                .map_err(|e| format!("failed to create GL context: {e}"))?
        };

        // Create GL surface
        let size = window.inner_size();
        let surface_attrs = SurfaceAttributesBuilder::<glutin::surface::WindowSurface>::new()
            .build(
                raw_window_handle,
                NonZeroU32::new(size.width.max(1)).expect("non-zero width"),
                NonZeroU32::new(size.height.max(1)).expect("non-zero height"),
            );

        let surface = unsafe {
            gl_display
                .create_window_surface(&gl_config, &surface_attrs)
                .map_err(|e| format!("failed to create window surface: {e}"))?
        };

        let gl_context = not_current_context
            .make_current(&surface)
            .map_err(|e| format!("failed to make GL context current: {e}"))?;

        // Load GL function pointers
        gl::load_with(|s| {
            gl_display
                .get_proc_address(std::ffi::CString::new(s).expect("valid CString").as_c_str())
                as *const _
        });

        let proc_address: ProcAddressLoader = {
            let display = gl_display.clone();
            Rc::new(move |s| {
                display
                    .get_proc_address(std::ffi::CString::new(s).expect("valid CString").as_c_str())
                    .cast()
            })
        };

        let render_target = WinitRenderTarget::new(Arc::clone(&window), surface, gl_context);

        render_target.set_swap_interval(vsync_enabled)?;

        let render_target_boxed: Box<dyn crate::rendering::RenderTarget> = Box::new(render_target);
        let mut gl_backend = GlBackend::new(
            render_target_boxed,
            proc_address,
            shader_path.as_deref(),
            app_context,
        )?;
        gl_backend.set_debugger_alpha(debugger_alpha);

        Ok(Self {
            gl_backend,
            render_target_window: window,
        })
    }

    /// Returns the current window size as `(width, height)` in pixels.
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
        crosshair: Option<Crosshair>,
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
    pub fn cycle_shader(&mut self) -> Option<String> {
        self.gl_backend.cycle_shader()
    }

    /// Updates the breakpoint list used by the debugger UI.
    pub fn update_breakpoints(&mut self, breakpoints: &BreakpointList) {
        self.gl_backend.update_breakpoints(breakpoints);
    }

    /// Returns the current watch addresses from the debugger.
    pub fn watch_addresses(&self) -> Vec<u16> {
        self.gl_backend.watch_addresses()
    }

    /// Replaces the debugger's watch address list.
    pub fn set_watch_addresses(&mut self, addresses: Vec<u16>) {
        self.gl_backend.set_watch_addresses(addresses);
    }

    /// Enables or disables fullscreen mode.
    pub fn set_fullscreen(&mut self, enabled: bool) -> Result<(), String> {
        self.gl_backend.set_fullscreen(enabled)
    }

    /// Enables or disables mouse confinement.
    pub fn set_mouse_grab(&mut self, enabled: bool) -> Result<(), String> {
        self.gl_backend.set_mouse_grab(enabled)
    }

    /// Notifies the render target that the physical window size has changed.
    ///
    /// Call this from `WindowEvent::Resized` and `WindowEvent::ScaleFactorChanged`
    /// so the glutin surface is resized to match the new physical dimensions.
    pub fn notify_resize(&mut self, w: u32, h: u32) {
        self.gl_backend.notify_resize(w, h);
    }

    /// Enables locked cursor mode (for SNES Mouse relative motion).
    ///
    /// Uses `CursorGrabMode::Locked` which hides the cursor and provides
    /// raw deltas via `DeviceEvent::MouseMotion`.
    pub fn set_mouse_grab_locked(&mut self) -> Result<(), String> {
        self.render_target_window
            .set_cursor_grab(winit::window::CursorGrabMode::Locked)
            .map_err(|e| format!("failed to set locked cursor grab: {e}"))
    }

    /// Returns a reference to the underlying window.
    pub fn window(&self) -> &Window {
        &self.render_target_window
    }

    /// Translates a winit keyboard event into a renderer [`InputEvent`].
    pub fn handle_key_event(&mut self, event: &KeyEvent) {
        if event.repeat {
            return;
        }
        let down = event.state == ElementState::Pressed;
        if let Some(key) = map_winit_key(&event.physical_key) {
            self.gl_backend.handle_input(&InputEvent::Key { key, down });
        }
    }

    /// Translates a winit cursor-moved event into a renderer mouse motion event.
    pub fn handle_cursor_moved(&mut self, position: winit::dpi::PhysicalPosition<f64>) {
        let scale_factor = self.render_target_window.scale_factor();
        self.gl_backend.handle_input(&InputEvent::MouseMotion {
            x: (position.x / scale_factor) as f32,
            y: (position.y / scale_factor) as f32,
        });
    }

    /// Translates a winit mouse button event into a renderer input event.
    pub fn handle_mouse_button(&mut self, button: MouseButton, state: ElementState) {
        let render_btn = match button {
            MouseButton::Left => Some(RenderMouseButton::Left),
            MouseButton::Right => Some(RenderMouseButton::Right),
            MouseButton::Middle => Some(RenderMouseButton::Middle),
            _ => None,
        };
        if let Some(btn) = render_btn {
            self.gl_backend.handle_input(&InputEvent::MouseButton {
                button: btn,
                pressed: state == ElementState::Pressed,
            });
        }
    }

    /// Translates a winit mouse wheel event into a renderer scroll event.
    pub fn handle_mouse_wheel(&mut self, delta: MouseScrollDelta) {
        let (x, y) = match delta {
            MouseScrollDelta::LineDelta(x, y) => (x, y),
            MouseScrollDelta::PixelDelta(pos) => {
                let scale_factor = self.render_target_window.scale_factor();
                ((pos.x / scale_factor) as f32, (pos.y / scale_factor) as f32)
            }
        };
        self.gl_backend
            .handle_input(&InputEvent::MouseWheel { x, y });
    }

    /// Forwards text input to the renderer (for ImGui text fields).
    pub fn handle_text_input(&mut self, text: String) {
        self.gl_backend.handle_input(&InputEvent::TextInput(text));
    }
}

/// Maps winit physical key codes to ImGui keys for the renderer input layer.
fn map_winit_key(key: &PhysicalKey) -> Option<imgui::Key> {
    match key {
        PhysicalKey::Code(code) => match code {
            KeyCode::Tab => Some(imgui::Key::Tab),
            KeyCode::ArrowLeft => Some(imgui::Key::LeftArrow),
            KeyCode::ArrowRight => Some(imgui::Key::RightArrow),
            KeyCode::ArrowUp => Some(imgui::Key::UpArrow),
            KeyCode::ArrowDown => Some(imgui::Key::DownArrow),
            KeyCode::PageUp => Some(imgui::Key::PageUp),
            KeyCode::PageDown => Some(imgui::Key::PageDown),
            KeyCode::Home => Some(imgui::Key::Home),
            KeyCode::End => Some(imgui::Key::End),
            KeyCode::Insert => Some(imgui::Key::Insert),
            KeyCode::Delete => Some(imgui::Key::Delete),
            KeyCode::Backspace => Some(imgui::Key::Backspace),
            KeyCode::Space => Some(imgui::Key::Space),
            KeyCode::Enter => Some(imgui::Key::Enter),
            KeyCode::Escape => Some(imgui::Key::Escape),
            KeyCode::KeyA => Some(imgui::Key::A),
            KeyCode::KeyC => Some(imgui::Key::C),
            KeyCode::KeyV => Some(imgui::Key::V),
            KeyCode::KeyX => Some(imgui::Key::X),
            KeyCode::KeyY => Some(imgui::Key::Y),
            KeyCode::KeyZ => Some(imgui::Key::Z),
            KeyCode::F1 => Some(imgui::Key::F1),
            KeyCode::F5 => Some(imgui::Key::F5),
            KeyCode::F10 => Some(imgui::Key::F10),
            KeyCode::F11 => Some(imgui::Key::F11),
            _ => None,
        },
        _ => None,
    }
}

/// Selects the target display index for fullscreen mode.
///
/// Returns `None` if not in fullscreen mode, `Ok(Some(index))` for a valid
/// display, or `Err` if the requested display index is out of range.
fn select_target_display(
    fullscreen: bool,
    fullscreen_display: Option<i32>,
    monitor_count: usize,
) -> Result<Option<usize>, String> {
    if !fullscreen {
        return Ok(None);
    }

    if monitor_count == 0 {
        return Err("No monitors detected. Cannot enter fullscreen mode.".to_string());
    }

    let target = match fullscreen_display {
        Some(display) => display,
        None => {
            if monitor_count >= 2 {
                1
            } else {
                0
            }
        }
    };

    if target < 0 || (target as usize) >= monitor_count {
        return Err(format!(
            "Invalid --display {target}. Available displays: 0..{}",
            monitor_count - 1
        ));
    }

    Ok(Some(target as usize))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_winit_key_returns_correct_imgui_key_for_tab() {
        let key = PhysicalKey::Code(KeyCode::Tab);
        assert_eq!(map_winit_key(&key), Some(imgui::Key::Tab));
    }

    #[test]
    fn map_winit_key_returns_correct_imgui_key_for_arrows() {
        assert_eq!(
            map_winit_key(&PhysicalKey::Code(KeyCode::ArrowLeft)),
            Some(imgui::Key::LeftArrow)
        );
        assert_eq!(
            map_winit_key(&PhysicalKey::Code(KeyCode::ArrowRight)),
            Some(imgui::Key::RightArrow)
        );
        assert_eq!(
            map_winit_key(&PhysicalKey::Code(KeyCode::ArrowUp)),
            Some(imgui::Key::UpArrow)
        );
        assert_eq!(
            map_winit_key(&PhysicalKey::Code(KeyCode::ArrowDown)),
            Some(imgui::Key::DownArrow)
        );
    }

    #[test]
    fn map_winit_key_returns_none_for_unmapped_keys() {
        assert_eq!(map_winit_key(&PhysicalKey::Code(KeyCode::F12)), None);
    }

    #[test]
    fn map_winit_key_returns_correct_imgui_key_for_letters() {
        assert_eq!(
            map_winit_key(&PhysicalKey::Code(KeyCode::KeyA)),
            Some(imgui::Key::A)
        );
        assert_eq!(
            map_winit_key(&PhysicalKey::Code(KeyCode::KeyZ)),
            Some(imgui::Key::Z)
        );
    }

    #[test]
    fn map_winit_key_returns_none_for_unspecified_physical_key() {
        let key = PhysicalKey::Unidentified(winit::keyboard::NativeKeyCode::Unidentified);
        assert_eq!(map_winit_key(&key), None);
    }

    #[test]
    fn map_winit_key_maps_debugger_function_keys() {
        assert_eq!(
            map_winit_key(&PhysicalKey::Code(KeyCode::F5)),
            Some(imgui::Key::F5)
        );
        assert_eq!(
            map_winit_key(&PhysicalKey::Code(KeyCode::F10)),
            Some(imgui::Key::F10)
        );
        assert_eq!(
            map_winit_key(&PhysicalKey::Code(KeyCode::F11)),
            Some(imgui::Key::F11)
        );
    }

    #[test]
    fn select_target_display_returns_none_when_not_fullscreen() {
        assert_eq!(select_target_display(false, None, 2).unwrap(), None);
        assert_eq!(select_target_display(false, Some(1), 2).unwrap(), None);
    }

    #[test]
    fn select_target_display_defaults_to_1_with_multiple_monitors() {
        assert_eq!(
            select_target_display(true, None, 3).unwrap(),
            Some(1),
            "should default to display 1 when ≥2 monitors are available"
        );
    }

    #[test]
    fn select_target_display_defaults_to_0_with_single_monitor() {
        assert_eq!(
            select_target_display(true, None, 1).unwrap(),
            Some(0),
            "should default to display 0 when only 1 monitor available"
        );
    }

    #[test]
    fn select_target_display_uses_configured_display() {
        assert_eq!(
            select_target_display(true, Some(2), 4).unwrap(),
            Some(2),
            "should use the configured display index"
        );
    }

    #[test]
    fn select_target_display_rejects_out_of_range() {
        let result = select_target_display(true, Some(5), 3);
        assert!(result.is_err(), "should error for out-of-range display");
    }

    #[test]
    fn select_target_display_rejects_negative_index() {
        let result = select_target_display(true, Some(-1), 2);
        assert!(result.is_err(), "should error for negative display index");
    }

    #[test]
    fn select_target_display_errors_when_no_monitors_detected() {
        let result = select_target_display(true, None, 0);
        assert!(
            result.is_err(),
            "should error when no monitors are detected"
        );
    }
}
