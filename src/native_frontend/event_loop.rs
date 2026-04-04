//! Minimal native event loop for the NES emulator.
//!
//! Uses winit's `ApplicationHandler` to drive the emulation loop
//! with rendering via `NativeGlWrapper` and audio via `NativeAudio`.

use crate::app_context::SharedAppContext;
use crate::audio::NesAudio;
use crate::console::Nes;
use crate::debugging::Tracing;
use crate::input::Button;
use crate::native_frontend::audio::NativeAudio;
use crate::native_frontend::gl_wrapper::NativeGlWrapper;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::WindowId;

use std::time::{Duration, Instant};

/// Minimal native event loop that runs the NES emulator.
pub struct NativeEventLoop {
    app_context: SharedAppContext,
    nes: Nes,
    audio: Option<NativeAudio>,
    tracing: Tracing,

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

    fn handle_key(&mut self, key_code: KeyCode, pressed: bool, event_loop: &ActiveEventLoop) {
        match key_code {
            KeyCode::Escape => event_loop.exit(),

            // Volume controls
            KeyCode::F2 if pressed => {
                if let Some(ref audio) = self.audio {
                    audio.set_volume(audio.get_volume() + 0.1);
                }
            }
            KeyCode::F3 if pressed => {
                if let Some(ref audio) = self.audio {
                    audio.set_volume(audio.get_volume() - 0.1);
                }
            }

            // NES controller: player 1
            KeyCode::ArrowUp => self.nes.set_button(1, Button::Up, pressed),
            KeyCode::ArrowDown => self.nes.set_button(1, Button::Down, pressed),
            KeyCode::ArrowLeft => self.nes.set_button(1, Button::Left, pressed),
            KeyCode::ArrowRight => self.nes.set_button(1, Button::Right, pressed),
            KeyCode::KeyZ => self.nes.set_button(1, Button::A, pressed),
            KeyCode::KeyX => self.nes.set_button(1, Button::B, pressed),
            KeyCode::Enter => self.nes.set_button(1, Button::Start, pressed),
            KeyCode::ShiftRight => self.nes.set_button(1, Button::Select, pressed),

            _ => {}
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

            WindowEvent::KeyboardInput { event, .. } => {
                if event.repeat {
                    return;
                }
                let pressed = event.state == ElementState::Pressed;
                if let PhysicalKey::Code(key_code) = event.physical_key {
                    self.handle_key(key_code, pressed, event_loop);
                }

                // Forward to gl_wrapper for ImGui
                if let Some(ref mut gl) = self.gl_wrapper {
                    gl.handle_key_event(&event);
                }
            }

            WindowEvent::RedrawRequested => {
                // Run one frame of emulation
                self.run_frame();

                // Render
                if let Some(ref mut gl) = self.gl_wrapper {
                    gl.render(&self.nes, false, None, false, None);
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
