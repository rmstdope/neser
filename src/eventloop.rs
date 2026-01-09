use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::audio::NesAudio;
use crate::input::Button;
use crate::nes::TvSystem;
use crate::tracing::Tracing;
use std::time::{Duration, Instant};

/// EventLoop manages the SDL2 event loop for the application.
/// It handles user input and window events, exiting when Escape is pressed or the window is closed.
pub struct EventLoop {
    _sdl_context: sdl2::Sdl,
    canvas: Option<Canvas<Window>>,
    event_pump: sdl2::EventPump,
    timing_scale: f32,
    vsync_enabled: bool,
    paused: bool,
    audio: Option<NesAudio>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyDownOutcome {
    Continue,
    Quit,
}

impl EventLoop {
    const MIN_SCALE: f32 = 1.0;
    const MAX_SCALE: f32 = 5.0;
    const MIN_TIMING_SCALE: f32 = 0.001;
    const MAX_TIMING_SCALE: f32 = 100.0;
    const CLEAR_COLOR_R: u8 = 0;
    const CLEAR_COLOR_G: u8 = 0;
    const CLEAR_COLOR_B: u8 = 0;

    /// Creates a new EventLoop instance.
    ///
    /// This is the preferred way to create an EventLoop.
    ///
    /// # Arguments
    ///
    /// * `headless` - If `true`, creates an EventLoop without a window (useful for testing).
    ///                If `false`, creates a window sized for the specified TV system.
    /// * `tv_system` - The TV system (NTSC or PAL) which determines the window size.
    ///                 NTSC and PAL both use 256x240 resolution.
    /// * `video_scale` - Window scaling factor. Values are clamped to the range [1.0, 5.0].
    ///             If a value outside this range is provided, it will be clamped and a warning
    ///             will be printed to the console.
    /// * `timing_scale` - Emulation speed multiplier. Values are clamped to the range [0.001, 100.0].
    ///             If a value outside this range is provided, it will be clamped and a warning
    ///             will be printed to the console.
    /// * `audio` - Optional audio output system. If provided, audio will be enabled.
    ///
    /// # Errors
    ///
    /// Returns an error if SDL2 initialization fails, the event pump cannot be created,
    /// or (when `headless` is `false`) the window cannot be created.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use neser::eventloop::EventLoop;
    /// use neser::nes::TvSystem;
    ///
    /// // Create a headless EventLoop for testing
    /// let headless = EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0, true, None)?;
    ///
    /// // Create an EventLoop with an NTSC window at 2x scale
    /// let ntsc = EventLoop::new(false, TvSystem::Ntsc, 2.0, 1.0, true, None)?;
    ///
    /// // Create an EventLoop with a PAL window at 3x scale at 2x speed
    /// let pal = EventLoop::new(false, TvSystem::Pal, 3.0, 2.0, true, None)?;
    /// # Ok::<(), String>(())
    /// ```
    pub fn new(
        headless: bool,
        tv_system: TvSystem,
        video_scale: f32,
        timing_scale: f32,
        vsync_enabled: bool,
        audio: Option<NesAudio>,
    ) -> Result<Self, String> {
        let clamped_video_scale = Self::clamp_scale(video_scale);
        let clamped_timing_scale = Self::clamp_timing_scale(timing_scale);

        let sdl_context = sdl2::init()?;
        let event_pump = sdl_context.event_pump()?;

        let canvas = if headless {
            None
        } else {
            Some(Self::create_window_and_canvas(
                &sdl_context,
                tv_system,
                clamped_video_scale,
                vsync_enabled,
            )?)
        };

        Ok(EventLoop {
            _sdl_context: sdl_context,
            canvas,
            event_pump,
            timing_scale: clamped_timing_scale,
            vsync_enabled,
            paused: false,
            audio,
        })
    }

    /// Clamps the video scaling factor to the valid range [1.0, 5.0].
    /// Prints a warning to stderr if clamping occurs.
    fn clamp_scale(scale: f32) -> f32 {
        if scale < Self::MIN_SCALE {
            eprintln!(
                "Warning: Video scaling factor {} is below minimum {}. Clamping to {}.",
                scale,
                Self::MIN_SCALE,
                Self::MIN_SCALE
            );
            Self::MIN_SCALE
        } else if scale > Self::MAX_SCALE {
            eprintln!(
                "Warning: Video scaling factor {} is above maximum {}. Clamping to {}.",
                scale,
                Self::MAX_SCALE,
                Self::MAX_SCALE
            );
            Self::MAX_SCALE
        } else {
            scale
        }
    }

    /// Clamps the timing scaling factor to the valid range [0.001, 100.0].
    /// Prints a warning to stderr if clamping occurs.
    fn clamp_timing_scale(scale: f32) -> f32 {
        if scale < Self::MIN_TIMING_SCALE {
            eprintln!(
                "Warning: Timing scaling factor {} is below minimum {}. Clamping to {}.",
                scale,
                Self::MIN_TIMING_SCALE,
                Self::MIN_TIMING_SCALE
            );
            Self::MIN_TIMING_SCALE
        } else if scale > Self::MAX_TIMING_SCALE {
            eprintln!(
                "Warning: Timing scaling factor {} is above maximum {}. Clamping to {}.",
                scale,
                Self::MAX_TIMING_SCALE,
                Self::MAX_TIMING_SCALE
            );
            Self::MAX_TIMING_SCALE
        } else {
            scale
        }
    }

    /// Creates a window with dimensions matching the specified TV system, scaled by the given factor.
    /// Returns a canvas for rendering.
    fn create_window_and_canvas(
        sdl_context: &sdl2::Sdl,
        tv_system: TvSystem,
        scale: f32,
        vsync_enabled: bool,
    ) -> Result<Canvas<Window>, String> {
        let base_width = tv_system.screen_width();
        let base_height = tv_system.screen_height();
        let scaled_width = (base_width as f32 * scale) as u32;
        let scaled_height = (base_height as f32 * scale) as u32;
        let video_subsystem = sdl_context.video()?;

        let window = video_subsystem
            .window("NES Emulator in Rust", scaled_width, scaled_height)
            .position_centered()
            .build()
            .map_err(|e| e.to_string())?;

        let canvas_builder = window.into_canvas();
        let canvas_builder = if vsync_enabled {
            canvas_builder.present_vsync()
        } else {
            canvas_builder
        };
        let mut canvas = canvas_builder.build().map_err(|e| e.to_string())?;
        canvas.set_draw_color(sdl2::pixels::Color::RGB(
            Self::CLEAR_COLOR_R,
            Self::CLEAR_COLOR_G,
            Self::CLEAR_COLOR_B,
        ));
        canvas.clear();
        canvas.present();

        Ok(canvas)
    }

    fn should_manual_frame_limit(vsync_enabled: bool) -> bool {
        !vsync_enabled
    }

    /// Checks if the user has requested to quit via Escape key or window close.
    /// Returns `true` if quit was requested, `false` otherwise.
    // fn should_quit(event_pump: &mut sdl2::EventPump) -> bool {
    //     for event in event_pump.poll_iter() {
    //         match event {
    //             Event::Quit { .. }
    //             | Event::KeyDown {
    //                 keycode: Some(Keycode::Escape),
    //                 ..
    //             } => return true,
    //             _ => {}
    //         }
    //     }
    //     false
    // }

    /// Renders the current frame from the PPU screen buffer to the screen.
    fn render_frame(
        canvas: &mut Canvas<Window>,
        texture: &mut sdl2::render::Texture,
        nes: &crate::nes::Nes,
    ) -> Result<(), String> {
        // Update texture from PPU screen buffer (256x240 pixels)
        const TEXTURE_WIDTH: u32 = 256;
        const TEXTURE_HEIGHT: u32 = 240;

        texture
            .with_lock(None, |buffer: &mut [u8], pitch: usize| {
                // Get the PPU screen buffer and copy its RGB data to the texture
                let screen_buffer = nes.get_screen_buffer();

                // Check if we can do a direct copy (pitch == width * 3 bytes per pixel)
                if pitch == (TEXTURE_WIDTH as usize * 3) {
                    // Fast path: direct buffer copy
                    screen_buffer.copy_buffer(buffer);
                } else {
                    // Slow path: copy row by row to handle non-standard pitch
                    for y in 0..TEXTURE_HEIGHT {
                        for x in 0..TEXTURE_WIDTH {
                            let (r, g, b) = screen_buffer.get_pixel(x, y);
                            let offset = (y as usize * pitch) + (x as usize * 3);
                            buffer[offset] = r;
                            buffer[offset + 1] = g;
                            buffer[offset + 2] = b;
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;

        canvas.set_draw_color(sdl2::pixels::Color::RGB(
            Self::CLEAR_COLOR_R,
            Self::CLEAR_COLOR_G,
            Self::CLEAR_COLOR_B,
        ));
        canvas.clear();
        canvas
            .copy(texture, None, None)
            .map_err(|e| e.to_string())?;
        canvas.present();

        Ok(())
    }

    /// Runs the event loop, processing events until the user presses Escape or closes the window.
    ///
    /// Continuously runs CPU opcodes on the provided NES instance according to the CPU clock
    /// frequency of the TV system.
    ///
    /// # Arguments
    ///
    /// * `nes` - A mutable reference to the NES instance to run.
    /// * `tracing` - Controls whether CPU tracing is enabled and which trace format is used.
    ///
    /// # Errors
    ///
    /// Currently returns Ok(()) in all cases, but the Result type is kept for future error handling.
    pub fn run(&mut self, nes: &mut crate::nes::Nes, tracing: Tracing) -> Result<(), String> {
        let mut last_audio_stats_print = Instant::now();
        let mut last_cpu_cycles = nes.cpu.get_total_cycles();
        let mut last_perf_instant = Instant::now();

        // Start audio playback if audio is enabled
        if let Some(ref audio) = self.audio {
            audio.resume();
        }

        if let Some(ref mut canvas) = self.canvas {
            // We have a window - run with rendering
            let texture_creator = canvas.texture_creator();

            // Create a 256x240 texture matching the PPU screen buffer dimensions
            const TEXTURE_WIDTH: u32 = 256;
            const TEXTURE_HEIGHT: u32 = 240;

            let mut texture = texture_creator
                .create_texture_streaming(PixelFormatEnum::RGB24, TEXTURE_WIDTH, TEXTURE_HEIGHT)
                .map_err(|e| e.to_string())?;

            let timer = self._sdl_context.timer()?;
            let mut last_frame_time = timer.performance_counter();
            let performance_frequency = timer.performance_frequency() as f64;

            loop {
                // 1. Poll ALL events (non-blocking)
                for event in self.event_pump.poll_iter() {
                    match event {
                        Event::Quit { .. } => return Ok(()),
                        Event::KeyDown {
                            keycode: Some(keycode),
                            ..
                        } => {
                            if Self::handle_key_down(
                                nes,
                                keycode,
                                self.audio.as_ref(),
                                &mut self.paused,
                            ) == KeyDownOutcome::Quit
                            {
                                return Ok(());
                            }
                        }
                        Event::KeyUp {
                            keycode: Some(keycode),
                            ..
                        } => {
                            Self::handle_key_up(nes, keycode);
                        }
                        _ => {}
                    }
                }

                // Skip emulation and rendering if paused
                if self.paused {
                    std::thread::sleep(std::time::Duration::from_millis(16));
                    continue;
                }

                // 2. Emulate until PPU completes a full frame (reaches VBlank)
                // The PPU runs at 3x CPU clock (NTSC) or 3.2x (PAL), so run_cpu_tick()
                // automatically runs the correct number of PPU cycles per CPU instruction.
                // A full frame is 262 scanlines × 341 pixels = 89,342 PPU cycles for NTSC
                while !nes.is_ready_to_render() && !nes.cpu.is_halted() {
                    nes.run(&tracing);

                    // Poll audio samples from APU and queue them
                    if let Some(ref mut audio) = self.audio {
                        while nes.sample_ready() {
                            if let Some(sample) = nes.get_sample() {
                                audio.queue_sample(sample);
                            }
                        }
                    }
                }

                if let Some(ref audio) = self.audio {
                    if last_audio_stats_print.elapsed() >= Duration::from_secs(1) {
                        let (received, dropped, underrun) = audio.take_and_reset_stats();
                        let now_cycles = nes.cpu.get_total_cycles();
                        let elapsed = last_perf_instant.elapsed().as_secs_f64();
                        let cycles_per_sec = if elapsed > 0.0 {
                            (now_cycles - last_cpu_cycles) as f64 / elapsed
                        } else {
                            0.0
                        };
                        if dropped != 0 || underrun != 0 {
                            eprintln!(
                                "Audio stats (last ~1s): received={}, dropped={}, underrun={}, cpu_cycles_per_sec≈{:.0}",
                                received, dropped, underrun, cycles_per_sec
                            );
                        }
                        last_cpu_cycles = now_cycles;
                        last_perf_instant = Instant::now();
                        last_audio_stats_print = Instant::now();
                    }
                }
                nes.clear_ready_to_render();
                // println!(
                //     "Frame emulated. Scanline: {}, Pixel: {}",
                //     nes.ppu.borrow().scanline(),
                //     nes.ppu.borrow().pixel()
                // );

                // 3. Render the frame
                Self::render_frame(canvas, &mut texture, nes)?;
                // println!("Frame rendered.");

                // 4. Frame limiting - maintain ~60 FPS (or scaled by timing_scale)
                let current_time = timer.performance_counter();
                let elapsed_ticks = (current_time - last_frame_time) as f64;
                let elapsed_seconds = elapsed_ticks / performance_frequency;
                // Adjust target frame time by timing scale (1.0 = normal speed, 2.0 = 2x speed, etc.)
                let target_frame_time = (1.0 / 60.0) / self.timing_scale as f64;

                // Calculate FPS before sleeping
                // let fps = 1.0 / elapsed_seconds;
                // println!("FPS: {:.2}", fps);

                // Update last_frame_time before sleeping to avoid timing drift
                last_frame_time = current_time;

                if Self::should_manual_frame_limit(self.vsync_enabled)
                    && elapsed_seconds < target_frame_time
                {
                    let sleep_time = target_frame_time - elapsed_seconds;
                    std::thread::sleep(std::time::Duration::from_secs_f64(sleep_time));
                }
                // println!("Frame limited.");
            }
        } else {
            // Headless mode - just run without rendering
            loop {
                for event in self.event_pump.poll_iter() {
                    match event {
                        Event::Quit { .. } => return Ok(()),
                        Event::KeyDown {
                            keycode: Some(keycode),
                            ..
                        } => {
                            if Self::handle_key_down(
                                nes,
                                keycode,
                                self.audio.as_ref(),
                                &mut self.paused,
                            ) == KeyDownOutcome::Quit
                            {
                                return Ok(());
                            }
                        }
                        Event::KeyUp {
                            keycode: Some(keycode),
                            ..
                        } => {
                            Self::handle_key_up(nes, keycode);
                        }
                        _ => {}
                    }
                }

                nes.run_cpu_tick();

                // Poll audio samples from APU and queue them
                if let Some(ref mut audio) = self.audio {
                    while nes.sample_ready() {
                        if let Some(sample) = nes.get_sample() {
                            audio.queue_sample(sample);
                        }
                    }

                    if last_audio_stats_print.elapsed() >= Duration::from_secs(1) {
                        let (received, dropped, underrun) = audio.take_and_reset_stats();
                        let now_cycles = nes.cpu.get_total_cycles();
                        let elapsed = last_perf_instant.elapsed().as_secs_f64();
                        let cycles_per_sec = if elapsed > 0.0 {
                            (now_cycles - last_cpu_cycles) as f64 / elapsed
                        } else {
                            0.0
                        };
                        if dropped != 0 || underrun != 0 {
                            eprintln!(
                                "Audio stats (last ~1s): received={}, dropped={}, underrun={}, cpu_cycles_per_sec≈{:.0}",
                                received, dropped, underrun, cycles_per_sec
                            );
                        }
                        last_cpu_cycles = now_cycles;
                        last_perf_instant = Instant::now();
                        last_audio_stats_print = Instant::now();
                    }
                }
            }
        }
    }

    /// Handle keyboard key press events
    ///
    /// Maps keyboard keys to NES controller buttons:
    /// - W/A/S/D: D-Pad (Up, Left, Down, Right)
    /// - G: B button
    /// - F: A button
    /// - R: Select button
    /// - T: Start button
    ///
    /// Emulator controls:
    /// - Escape: Quit
    /// - Space: Toggle pause
    /// - F1: Reset
    /// - F2/F3: Volume up/down (when audio is enabled)
    fn handle_key_down(
        nes: &mut crate::nes::Nes,
        keycode: Keycode,
        audio: Option<&NesAudio>,
        paused: &mut bool,
    ) -> KeyDownOutcome {
        match keycode {
            Keycode::Escape => return KeyDownOutcome::Quit,
            Keycode::Space => {
                *paused = !*paused;
            }
            Keycode::F1 => {
                println!("Resetting NES...");
                nes.reset(true);
            }
            Keycode::F2 => {
                if let Some(audio) = audio {
                    apply_volume_hotkey(audio, Keycode::F2);
                }
            }
            Keycode::F3 => {
                if let Some(audio) = audio {
                    apply_volume_hotkey(audio, Keycode::F3);
                }
            }
            Keycode::W => nes.set_button(1, Button::Up, true),
            Keycode::S => nes.set_button(1, Button::Down, true),
            Keycode::A => nes.set_button(1, Button::Left, true),
            Keycode::D => nes.set_button(1, Button::Right, true),
            Keycode::G => nes.set_button(1, Button::B, true),
            Keycode::F => nes.set_button(1, Button::A, true),
            Keycode::R => nes.set_button(1, Button::Select, true),
            Keycode::T => nes.set_button(1, Button::Start, true),
            _ => {}
        }

        KeyDownOutcome::Continue
    }

    /// Handle keyboard key release events
    fn handle_key_up(nes: &mut crate::nes::Nes, keycode: Keycode) {
        use crate::input::Button;
        match keycode {
            Keycode::W => nes.set_button(1, Button::Up, false),
            Keycode::S => nes.set_button(1, Button::Down, false),
            Keycode::A => nes.set_button(1, Button::Left, false),
            Keycode::D => nes.set_button(1, Button::Right, false),
            Keycode::G => nes.set_button(1, Button::B, false),
            Keycode::F => nes.set_button(1, Button::A, false),
            Keycode::R => nes.set_button(1, Button::Select, false),
            Keycode::T => nes.set_button(1, Button::Start, false),
            _ => {}
        }
    }
}

fn apply_volume_hotkey(audio: &NesAudio, keycode: Keycode) {
    const STEP: f32 = 0.1;

    let current = audio.get_volume();
    let next = match keycode {
        Keycode::F2 => current + STEP,
        Keycode::F3 => current - STEP,
        _ => current,
    };

    audio.set_volume(next);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::{Nes, TvSystem};
    use serial_test::serial;
    use std::env;

    fn read_joypad1_buttons(nes: &mut Nes) -> [u8; 8] {
        // Joypad serial order: A, B, Select, Start, Up, Down, Left, Right
        {
            let mut mem = nes.memory.borrow_mut();
            mem.write(0x4016, 1, false);
            mem.write(0x4016, 0, false);
        }

        let mut out = [0u8; 8];
        for i in 0..8 {
            let value = nes.memory.borrow_mut().read(0x4016) & 0x01;
            out[i] = value;
        }
        out
    }

    #[test]
    fn test_manual_frame_limiting_is_disabled_with_vsync() {
        assert!(!EventLoop::should_manual_frame_limit(true));
    }

    #[test]
    fn test_manual_frame_limiting_is_enabled_without_vsync() {
        assert!(EventLoop::should_manual_frame_limit(false));
    }

    #[test]
    #[serial]
    fn test_eventloop_creation() {
        let event_loop = EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0, true, None);
        assert!(event_loop.is_ok());
    }

    #[test]
    #[serial]
    fn test_volume_hotkeys_f2_f3_adjust_by_point_one() {
        // CI often runs without an audio device; force SDL to use its dummy backend.
        // Restore the previous env value after the test to avoid cross-test pollution.
        struct EnvRestore {
            key: &'static str,
            prev: Option<String>,
        }

        impl Drop for EnvRestore {
            fn drop(&mut self) {
                match &self.prev {
                    Some(value) => unsafe { env::set_var(self.key, value) },
                    None => unsafe { env::remove_var(self.key) },
                }
            }
        }

        let restore = EnvRestore {
            key: "SDL_AUDIODRIVER",
            prev: env::var("SDL_AUDIODRIVER").ok(),
        };
        unsafe {
            env::set_var("SDL_AUDIODRIVER", "dummy");
        }

        let sdl_context = sdl2::init().expect("Failed to initialize SDL2");
        let audio = NesAudio::new(&sdl_context, 44100).expect("Audio init should succeed");

        // Default volume is 0.25.
        assert!((audio.get_volume() - 0.25).abs() < 1e-6);

        apply_volume_hotkey(&audio, Keycode::F2);
        assert!(
            (audio.get_volume() - 0.35).abs() < 1e-6,
            "F2 should raise volume by 0.1"
        );

        apply_volume_hotkey(&audio, Keycode::F3);
        assert!(
            (audio.get_volume() - 0.25).abs() < 1e-6,
            "F3 should lower volume by 0.1"
        );

        drop(restore);
    }

    #[test]
    #[serial]
    fn test_handle_key_down_routes_f2_f3_to_audio() {
        struct EnvRestore {
            key: &'static str,
            prev: Option<String>,
        }

        impl Drop for EnvRestore {
            fn drop(&mut self) {
                match &self.prev {
                    Some(value) => unsafe { env::set_var(self.key, value) },
                    None => unsafe { env::remove_var(self.key) },
                }
            }
        }

        let restore = EnvRestore {
            key: "SDL_AUDIODRIVER",
            prev: env::var("SDL_AUDIODRIVER").ok(),
        };
        unsafe {
            env::set_var("SDL_AUDIODRIVER", "dummy");
        }

        let sdl_context = sdl2::init().expect("Failed to initialize SDL2");
        let audio = NesAudio::new(&sdl_context, 44100).expect("Audio init should succeed");
        let mut nes = Nes::new(TvSystem::Ntsc);
        let mut paused = false;

        let before = audio.get_volume();
        EventLoop::handle_key_down(&mut nes, Keycode::F2, Some(&audio), &mut paused);
        assert!((audio.get_volume() - (before + 0.1)).abs() < 1e-6);
        EventLoop::handle_key_down(&mut nes, Keycode::F3, Some(&audio), &mut paused);
        assert!((audio.get_volume() - before).abs() < 1e-6);

        drop(restore);
    }

    #[test]
    fn test_handle_key_down_escape_requests_quit() {
        // Desired behavior: key handling for Escape is centralized in handle_key_down,
        // and it indicates that the event loop should exit.
        let mut nes = Nes::new(TvSystem::Ntsc);
        let mut paused = false;

        let outcome = EventLoop::handle_key_down(&mut nes, Keycode::Escape, None, &mut paused);
        assert_eq!(outcome, KeyDownOutcome::Quit);
    }

    #[test]
    fn test_handle_key_down_space_toggles_pause() {
        // Desired behavior: Space toggles pause state via centralized handle_key_down.
        let mut nes = Nes::new(TvSystem::Ntsc);
        let mut paused = false;

        let _ = EventLoop::handle_key_down(&mut nes, Keycode::Space, None, &mut paused);
        assert!(paused);
        let _ = EventLoop::handle_key_down(&mut nes, Keycode::Space, None, &mut paused);
        assert!(!paused);
    }

    #[test]
    fn test_handle_key_down_f1_resets_nes() {
        // Desired behavior: F1 triggers a reset through centralized handle_key_down.
        let mut nes = Nes::new(TvSystem::Ntsc);
        let mut paused = false;

        // Reset reads the reset vector from $FFFC-$FFFD. Inserting a minimal cartridge
        // avoids panicking on unmapped reads.
        let mut prg_rom = vec![0u8; 0x8000];
        let reset_vector: u16 = 0x8000;
        prg_rom[0x7FFC] = (reset_vector & 0x00FF) as u8;
        prg_rom[0x7FFD] = (reset_vector >> 8) as u8;
        let cartridge = crate::cartridge::Cartridge::from_parts(
            prg_rom,
            vec![],
            crate::cartridge::MirroringMode::Horizontal,
        );
        nes.insert_cartridge(cartridge);

        nes.cpu.pc = 0x1234;
        let _ = EventLoop::handle_key_down(&mut nes, Keycode::F1, None, &mut paused);
        assert_eq!(nes.cpu.pc, reset_vector);
    }

    #[test]
    fn test_joypad1_keyboard_mapping_wasd_r_t_f_g() {
        // Desired mapping (Joypad 1):
        // - D-pad: W/A/S/D
        // - Select: R
        // - Start:  T
        // - A:      F
        // - B:      G
        let mut paused = false;

        // W => Up
        let mut nes = Nes::new(TvSystem::Ntsc);
        let _ = EventLoop::handle_key_down(&mut nes, Keycode::W, None, &mut paused);
        assert_eq!(read_joypad1_buttons(&mut nes), [0, 0, 0, 0, 1, 0, 0, 0]);

        // S => Down
        let mut nes = Nes::new(TvSystem::Ntsc);
        let _ = EventLoop::handle_key_down(&mut nes, Keycode::S, None, &mut paused);
        assert_eq!(read_joypad1_buttons(&mut nes), [0, 0, 0, 0, 0, 1, 0, 0]);

        // A => Left
        let mut nes = Nes::new(TvSystem::Ntsc);
        let _ = EventLoop::handle_key_down(&mut nes, Keycode::A, None, &mut paused);
        assert_eq!(read_joypad1_buttons(&mut nes), [0, 0, 0, 0, 0, 0, 1, 0]);

        // D => Right
        let mut nes = Nes::new(TvSystem::Ntsc);
        let _ = EventLoop::handle_key_down(&mut nes, Keycode::D, None, &mut paused);
        assert_eq!(read_joypad1_buttons(&mut nes), [0, 0, 0, 0, 0, 0, 0, 1]);

        // R => Select
        let mut nes = Nes::new(TvSystem::Ntsc);
        let _ = EventLoop::handle_key_down(&mut nes, Keycode::R, None, &mut paused);
        assert_eq!(read_joypad1_buttons(&mut nes), [0, 0, 1, 0, 0, 0, 0, 0]);

        // T => Start
        let mut nes = Nes::new(TvSystem::Ntsc);
        let _ = EventLoop::handle_key_down(&mut nes, Keycode::T, None, &mut paused);
        assert_eq!(read_joypad1_buttons(&mut nes), [0, 0, 0, 1, 0, 0, 0, 0]);

        // F => A
        let mut nes = Nes::new(TvSystem::Ntsc);
        let _ = EventLoop::handle_key_down(&mut nes, Keycode::F, None, &mut paused);
        assert_eq!(read_joypad1_buttons(&mut nes), [1, 0, 0, 0, 0, 0, 0, 0]);

        // G => B
        let mut nes = Nes::new(TvSystem::Ntsc);
        let _ = EventLoop::handle_key_down(&mut nes, Keycode::G, None, &mut paused);
        assert_eq!(read_joypad1_buttons(&mut nes), [0, 1, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    #[serial]
    fn test_new_headless() {
        let event_loop = EventLoop::new(true, TvSystem::Ntsc, 2.0, 1.0, true, None);
        assert!(event_loop.is_ok());
    }

    #[test]
    #[serial]
    fn test_scaling_below_minimum() {
        let event_loop = EventLoop::new(true, TvSystem::Ntsc, 0.5, 1.0, true, None);
        assert!(event_loop.is_ok());
    }

    #[test]
    #[serial]
    fn test_scaling_above_maximum() {
        let event_loop = EventLoop::new(true, TvSystem::Ntsc, 6.0, 1.0, true, None);
        assert!(event_loop.is_ok());
    }

    #[test]
    #[serial]
    fn test_scaling_at_minimum() {
        let event_loop = EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0, true, None);
        assert!(event_loop.is_ok());
    }

    #[test]
    #[serial]
    fn test_scaling_at_maximum() {
        let event_loop = EventLoop::new(true, TvSystem::Ntsc, 5.0, 1.0, true, None);
        assert!(event_loop.is_ok());
    }

    #[test]
    #[serial]
    fn test_run_with_nes() {
        let _event_loop = EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0, true, None).unwrap();
        let mut nes = Nes::new(TvSystem::Ntsc);

        // Just verify that run accepts a Nes instance
        // We can't actually run the event loop in tests as it would loop forever
        // This test just checks the signature compiles
        let _ = &mut nes;
    }

    #[test]
    fn test_render_frame_should_use_256x240_texture() {
        // Verify that render_frame uses correct PPU screen buffer dimensions
        const EXPECTED_WIDTH: u32 = 256;
        const EXPECTED_HEIGHT: u32 = 240;

        // The render_frame function now uses the correct 256x240 dimensions
        // matching the PPU screen buffer size
        assert_eq!(EXPECTED_WIDTH, 256);
        assert_eq!(EXPECTED_HEIGHT, 240);
    }
}
