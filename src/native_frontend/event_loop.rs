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

use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
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
        Self {
            app_context,
            nes,
            audio,
            tracing,
            state: NativeAppState::new(),
            gl_wrapper: None,
            last_audio_stats_print: Instant::now(),
            initialized: false,
        }
    }

    pub fn run(mut self) -> Result<(), String> {
        let event_loop =
            EventLoop::new().map_err(|e| format!("Failed to create event loop: {e}"))?;
        event_loop.set_control_flow(ControlFlow::Poll);
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
            }

            WindowEvent::RedrawRequested => {
                // Run one frame of emulation
                self.run_frame();

                // Render
                if let Some(ref mut gl) = self.gl_wrapper {
                    let overlay = self.state.overlay_text(&self.nes);
                    gl.render(
                        &self.nes,
                        self.state.debugger_open,
                        overlay.as_deref(),
                        false,
                        None,
                    );
                }
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Request a redraw for every iteration (polling mode)
        if let Some(ref gl) = self.gl_wrapper {
            gl.window().request_redraw();
        }
    }
}
