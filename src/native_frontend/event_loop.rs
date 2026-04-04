//! Native event loop for the NES emulator.
//!
//! Uses winit's `ApplicationHandler` to drive the emulation loop
//! with rendering via `NativeGlWrapper` and audio via `NativeAudio`.

use crate::app_context::SharedAppContext;
use crate::audio::NesAudio;
use crate::console::Nes;
use crate::debugging::Tracing;
use crate::native_frontend::app_state::NativeAppState;
use crate::native_frontend::audio::NativeAudio;
use crate::native_frontend::gl_wrapper::NativeGlWrapper;
use crate::native_frontend::keyboard::{self, KeyOutcome};
use crate::native_frontend::mouse;

use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::WindowId;

use std::time::{Duration, Instant};

/// Native event loop that runs the NES emulator using winit + glutin.
pub struct NativeEventLoop {
    app_context: SharedAppContext,
    nes: Nes,
    audio: Option<NativeAudio>,
    tracing: Tracing,
    state: NativeAppState,

    // Initialized on resume (when the window is ready)
    gl_wrapper: Option<NativeGlWrapper>,
    last_audio_stats_print: Instant,
    initialized: bool,
}

impl NativeEventLoop {
    pub fn new(
        app_context: SharedAppContext,
        nes: Nes,
        audio: Option<NativeAudio>,
        tracing: Tracing,
    ) -> Self {
        let fullscreen = app_context.borrow().config().fullscreen;
        Self {
            app_context,
            nes,
            audio,
            tracing,
            state: NativeAppState {
                fullscreen,
                window_focused: true,
                ..NativeAppState::default()
            },
            gl_wrapper: None,
            last_audio_stats_print: Instant::now(),
            initialized: false,
        }
    }

    pub fn run(mut self) -> Result<(), String> {
        let event_loop =
            EventLoop::new().map_err(|e| format!("Failed to create event loop: {e}"))?;
        event_loop
            .run_app(&mut self)
            .map_err(|e| format!("Event loop error: {e}"))
    }

    fn initialize_audio(&mut self) {
        if let Some(ref mut audio) = self.audio {
            audio.prime_startup(2048);
            audio.resume();
        }
    }

    fn run_frame(&mut self) {
        if self.state.paused {
            return;
        }

        // Emulate until PPU completes a frame
        while !self.nes.is_ready_to_render() && !self.nes.cpu_ref().is_halted() {
            self.nes.run(&self.tracing);

            // Drain audio samples from APU
            if let Some(ref mut audio) = self.audio {
                while self.nes.sample_ready() {
                    if let Some(sample) = self.nes.get_sample() {
                        audio.queue_sample(sample);
                    }
                }
            }
        }
        self.nes.clear_ready_to_render();

        // Log audio stats every second
        if let Some(ref audio) = self.audio
            && self.last_audio_stats_print.elapsed() >= Duration::from_secs(1)
        {
            let (received, dropped, underrun) = audio.take_and_reset_stats();
            if dropped != 0 || underrun != 0 {
                crate::debugging::log_info(format!(
                    "Audio stats (last ~1s): received={received}, dropped={dropped}, underrun={underrun}"
                ));
            }
            self.last_audio_stats_print = Instant::now();
        }
    }

    /// Synchronizes the actual mouse grab state with the desired state.
    ///
    /// Called once per frame to ensure grab/visibility stay in sync after
    /// cartridge switches, focus changes, or controller hot-swaps.
    fn sync_mouse_grab_state(&mut self) {
        let has_mouse = mouse::has_any_mouse_controller(&self.nes);
        let should_grab = crate::input::mouse_mapping::should_grab_mouse_input(
            has_mouse,
            self.state.window_focused,
            self.state.mouse_released_by_escape,
        );

        if self.state.mouse_grabbed != should_grab {
            if let Some(ref mut gl) = self.gl_wrapper {
                if should_grab {
                    let use_locked = crate::input::mouse_mapping::should_use_relative_mouse_mode(
                        true,
                        self.nes.has_snes_mouse(),
                    );
                    let _ = if use_locked {
                        gl.set_mouse_grab_locked()
                    } else {
                        gl.set_mouse_grab(true)
                    };
                    gl.window().set_cursor_visible(false);
                } else {
                    let _ = gl.set_mouse_grab(false);
                    gl.window().set_cursor_visible(true);
                }
            }
            self.state.mouse_grabbed = should_grab;
        }
    }
}

impl ApplicationHandler for NativeEventLoop {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gl_wrapper.is_some() {
            return;
        }

        match NativeGlWrapper::new(event_loop, self.app_context.clone()) {
            Ok(gl) => {
                self.gl_wrapper = Some(gl);
                if !self.initialized {
                    self.initialize_audio();
                    self.initialized = true;
                }
            }
            Err(e) => {
                eprintln!("Failed to create GL wrapper: {e}");
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Focused(focused) => {
                self.state.window_focused = focused;
                if !focused {
                    // Auto-release mouse on focus loss.
                    self.state.mouse_grabbed = false;
                    self.state.mouse_released_by_escape = true;
                    if let Some(ref mut gl) = self.gl_wrapper {
                        let _ = gl.set_mouse_grab(false);
                        gl.window().set_cursor_visible(true);
                    }
                }
            }

            WindowEvent::ModifiersChanged(mods) => {
                self.state.modifiers = mods.state();
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if event.repeat {
                    return;
                }

                if let Some(ref mut gl) = self.gl_wrapper {
                    gl.handle_key_event(&event);
                }

                let PhysicalKey::Code(key_code) = event.physical_key else {
                    return;
                };

                let mouse_grabbed_before = self.state.mouse_grabbed;

                if event.state == ElementState::Pressed {
                    let fullscreen_before = self.state.fullscreen;
                    let audio_ref: Option<&dyn NesAudio> =
                        self.audio.as_ref().map(|a| a as &dyn NesAudio);
                    let outcome = keyboard::handle_key_pressed(
                        &mut self.nes,
                        key_code,
                        &mut self.state,
                        audio_ref,
                    );
                    match outcome {
                        KeyOutcome::Quit => event_loop.exit(),
                        KeyOutcome::CycleShader => {
                            if let Some(ref mut gl) = self.gl_wrapper {
                                gl.cycle_shader();
                            }
                        }
                        KeyOutcome::Continue => {
                            if self.state.fullscreen != fullscreen_before {
                                if let Some(ref mut gl) = self.gl_wrapper {
                                    let _ = gl.set_fullscreen(self.state.fullscreen);
                                }
                            }
                        }
                    }
                } else {
                    keyboard::handle_key_released(&mut self.nes, key_code);
                }

                // If keyboard handler released the mouse grab (Escape), apply it.
                if mouse_grabbed_before && !self.state.mouse_grabbed {
                    if let Some(ref mut gl) = self.gl_wrapper {
                        let _ = gl.set_mouse_grab(false);
                        gl.window().set_cursor_visible(true);
                    }
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                // Forward to imgui/UI layer.
                if let Some(ref mut gl) = self.gl_wrapper {
                    gl.handle_cursor_moved(position);
                }

                // Route to NES controllers when mouse is grabbed.
                let has_mouse = mouse::has_any_mouse_controller(&self.nes);
                if has_mouse && self.state.mouse_grabbed {
                    let (w, h) = self
                        .gl_wrapper
                        .as_ref()
                        .map(|gl| gl.window_size())
                        .unwrap_or((320, 240));

                    let use_relative = crate::input::mouse_mapping::should_use_relative_mouse_mode(
                        true,
                        self.nes.has_snes_mouse(),
                    );

                    // In locked/relative mode, CursorMoved is unreliable —
                    // deltas come via DeviceEvent::MouseMotion instead.
                    if !use_relative {
                        let scale = self
                            .gl_wrapper
                            .as_ref()
                            .map(|gl| gl.window().scale_factor())
                            .unwrap_or(1.0);
                        let x = (position.x / scale) as i32;
                        let y = (position.y / scale) as i32;
                        self.state.last_zapper_position =
                            mouse::update_mouse_motion(&mut self.nes, x, y, w, h);
                    }
                }
            }

            WindowEvent::MouseInput { button, state, .. } => {
                // Forward to imgui/UI layer.
                if let Some(ref mut gl) = self.gl_wrapper {
                    gl.handle_mouse_button(button, state);
                }

                let has_mouse = mouse::has_any_mouse_controller(&self.nes);

                // Left-click grabs the mouse when a mouse controller is active.
                if has_mouse
                    && !self.state.mouse_grabbed
                    && state == ElementState::Pressed
                    && button == winit::event::MouseButton::Left
                {
                    self.state.mouse_released_by_escape = false;
                    let should_grab = crate::input::mouse_mapping::should_grab_mouse_input(
                        true,
                        self.state.window_focused,
                        false,
                    );
                    if should_grab {
                        self.state.mouse_grabbed = true;
                        if let Some(ref mut gl) = self.gl_wrapper {
                            let use_locked =
                                crate::input::mouse_mapping::should_use_relative_mouse_mode(
                                    true,
                                    self.nes.has_snes_mouse(),
                                );
                            let _ = if use_locked {
                                gl.set_mouse_grab_locked()
                            } else {
                                gl.set_mouse_grab(true)
                            };
                            gl.window().set_cursor_visible(false);
                        }
                    }
                }

                // Route button to NES controller if grabbed.
                if has_mouse && self.state.mouse_grabbed {
                    let btn = match button {
                        winit::event::MouseButton::Left => Some(mouse::MouseButton::Left),
                        winit::event::MouseButton::Right => Some(mouse::MouseButton::Right),
                        _ => None,
                    };
                    if let Some(btn) = btn {
                        mouse::update_mouse_button(
                            &mut self.nes,
                            btn,
                            state == ElementState::Pressed,
                        );
                    }
                }
            }

            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(ref mut gl) = self.gl_wrapper {
                    gl.handle_mouse_wheel(delta);
                }
            }

            WindowEvent::RedrawRequested => {
                // Run one frame of emulation
                self.run_frame();

                // Sync mouse grab state each frame.
                self.sync_mouse_grab_state();

                // Render
                if let Some(ref mut gl) = self.gl_wrapper {
                    let overlay = self.state.overlay_text(&self.nes);
                    let crosshair =
                        mouse::zapper_crosshair(&self.nes, self.state.last_zapper_position);
                    gl.render(
                        &self.nes,
                        self.state.debugger_open,
                        overlay.as_deref(),
                        false,
                        crosshair,
                    );
                }
            }

            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        // Raw mouse deltas for SNES Mouse relative motion (locked cursor mode).
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            if self.state.mouse_grabbed && self.nes.has_snes_mouse() {
                let (w, h) = self
                    .gl_wrapper
                    .as_ref()
                    .map(|gl| gl.window_size())
                    .unwrap_or((320, 240));
                mouse::apply_snes_mouse_relative_motion(&mut self.nes, dx as i32, dy as i32, w, h);
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(ref gl) = self.gl_wrapper {
            if self.state.paused {
                // Throttle to ~20fps while paused to avoid spinning the CPU/GPU.
                event_loop.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + Duration::from_millis(50),
                ));
            } else {
                event_loop.set_control_flow(ControlFlow::Poll);
            }
            gl.window().request_redraw();
        }
    }
}
