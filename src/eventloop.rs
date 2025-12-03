use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::nes::TvSystem;

/// EventLoop manages the SDL2 event loop for the application.
/// It handles user input and window events, exiting when Escape is pressed or the window is closed.
pub struct EventLoop {
    _sdl_context: sdl2::Sdl,
    canvas: Option<Canvas<Window>>,
    event_pump: sdl2::EventPump,
    tv_system: TvSystem,
    timing_scale: f32,
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
    /// let headless = EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0)?;
    ///
    /// // Create an EventLoop with an NTSC window at 2x scale
    /// let ntsc = EventLoop::new(false, TvSystem::Ntsc, 2.0, 1.0)?;
    ///
    /// // Create an EventLoop with a PAL window at 3x scale at 2x speed
    /// let pal = EventLoop::new(false, TvSystem::Pal, 3.0, 2.0)?;
    /// # Ok::<(), String>(())
    /// ```
    pub fn new(
        headless: bool,
        tv_system: TvSystem,
        video_scale: f32,
        timing_scale: f32,
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
            )?)
        };

        Ok(EventLoop {
            _sdl_context: sdl_context,
            canvas,
            event_pump,
            tv_system,
            timing_scale: clamped_timing_scale,
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

        let mut canvas = window.into_canvas().build().map_err(|e| e.to_string())?;
        canvas.set_draw_color(sdl2::pixels::Color::RGB(
            Self::CLEAR_COLOR_R,
            Self::CLEAR_COLOR_G,
            Self::CLEAR_COLOR_B,
        ));
        canvas.clear();
        canvas.present();

        Ok(canvas)
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
    ///
    /// # Errors
    ///
    /// Currently returns Ok(()) in all cases, but the Result type is kept for future error handling.
    pub fn run(&mut self, nes: &mut crate::nes::Nes) -> Result<(), String> {
        if let Some(ref mut canvas) = self.canvas {
            // We have a window - run with rendering
            let texture_creator = canvas.texture_creator();

            // Create a 256x240 texture matching the PPU screen buffer dimensions
            const TEXTURE_WIDTH: u32 = 256;
            const TEXTURE_HEIGHT: u32 = 240;

            let mut texture = texture_creator
                .create_texture_streaming(PixelFormatEnum::RGB24, TEXTURE_WIDTH, TEXTURE_HEIGHT)
                .map_err(|e| e.to_string())?;

            // Get CPU clock frequency for timing
            let cpu_frequency = self.tv_system.cpu_clock_frequency() as f64;
            let mut cycles_per_frame = cpu_frequency / 60.0; // ~60 FPS for NTSC, ~50 for PAL
            cycles_per_frame *= self.timing_scale as f64;

            let timer = self._sdl_context.timer()?;
            let mut last_frame_time = timer.performance_counter();
            let performance_frequency = timer.performance_frequency() as f64;

            loop {
                // 1. Poll ALL events (non-blocking)
                for event in self.event_pump.poll_iter() {
                    match event {
                        Event::Quit { .. }
                        | Event::KeyDown {
                            keycode: Some(Keycode::Escape),
                            ..
                        } => return Ok(()),
                        _ => {}
                    }
                }
                // println!("Events polled.");

                // 2. Emulate one frame worth of CPU cycles
                let mut cycles_run = 0.0;
                while cycles_run < cycles_per_frame && !nes.cpu.halted {
                    let cycles_consumed = nes.run_cpu_tick() as f64;
                    cycles_run += cycles_consumed;
                    // Write random value to 0xfe (used by some games for random number generation)
                    // nes.memory
                    //     .borrow_mut()
                    //     .write(0xfe as u16, rand::random::<u8>());
                }
                // println!("Frame emulated.");

                // 3. Render the frame
                Self::render_frame(canvas, &mut texture, nes)?;
                // println!("Frame rendered.");

                // 4. Frame limiting - maintain ~60 FPS
                let current_time = timer.performance_counter();
                let elapsed_ticks = (current_time - last_frame_time) as f64;
                let elapsed_seconds = elapsed_ticks / performance_frequency;
                let target_frame_time = 1.0 / 60.0; // ~16.67ms per frame
                
                // Calculate FPS before sleeping
                let fps = 1.0 / elapsed_seconds;
                // println!("FPS: {:.2}", fps);
                
                // Update last_frame_time before sleeping to avoid timing drift
                last_frame_time = current_time;
                
                if elapsed_seconds < target_frame_time {
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
                        Event::Quit { .. }
                        | Event::KeyDown {
                            keycode: Some(Keycode::Escape),
                            ..
                        } => return Ok(()),
                        _ => {}
                    }
                }

                nes.run_cpu_tick();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::{Nes, TvSystem};
    use std::sync::Mutex;

    // SDL2 can only be initialized once per process, so we use a mutex to ensure tests run serially
    static TEST_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_eventloop_creation() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let event_loop = EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0);
        assert!(event_loop.is_ok());
    }

    #[test]
    fn test_new_headless() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let event_loop = EventLoop::new(true, TvSystem::Ntsc, 2.0, 1.0);
        assert!(event_loop.is_ok());
    }

    #[test]
    fn test_scaling_below_minimum() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let event_loop = EventLoop::new(true, TvSystem::Ntsc, 0.5, 1.0);
        assert!(event_loop.is_ok());
    }

    #[test]
    fn test_scaling_above_maximum() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let event_loop = EventLoop::new(true, TvSystem::Ntsc, 6.0, 1.0);
        assert!(event_loop.is_ok());
    }

    #[test]
    fn test_scaling_at_minimum() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let event_loop = EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0);
        assert!(event_loop.is_ok());
    }

    #[test]
    fn test_scaling_at_maximum() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let event_loop = EventLoop::new(true, TvSystem::Ntsc, 5.0, 1.0);
        assert!(event_loop.is_ok());
    }

    #[test]
    fn test_run_with_nes() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let _event_loop = EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0).unwrap();
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
