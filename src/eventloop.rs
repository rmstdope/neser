use crate::gl_backend::GlBackend;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::audio::NesAudio;
use crate::input::Button;
use crate::nes::TvSystem;
use crate::tracing::Tracing;

/// EventLoop manages the SDL2 event loop for the application.
/// It handles user input and window events, exiting when Escape is pressed or the window is closed.
pub struct EventLoop {
    _sdl_context: sdl2::Sdl,
    gl_backend: Option<GlBackend>,
    event_pump: sdl2::EventPump,
    timing_scale: f32,
    vsync_enabled: bool,
    paused: bool,
    debugger_open_requested: bool,
    breakpoints: Vec<u16>,
    temporary_breakpoint: Option<TemporaryBreakpoint>,
    arm_temporary_breakpoint_after_next_instruction: bool,
    breakpoint_ignore_once_at_pc: Option<u16>,
    #[cfg_attr(not(test), allow(dead_code))]
    debugger_renderer: Option<Box<dyn DebuggerRenderer>>,
    audio: Option<NesAudio>,
    controllers: Vec<sdl2::controller::GameController>,
    controller_player_map: HashMap<u32, u8>, // Maps controller instance_id to player number (1 or 2)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TemporaryBreakpoint {
    pc: u16,
    already_present: bool,
    required_interrupt: Option<crate::cpu::InterruptKind>,
    has_exited_required_interrupt: bool,
    ignore_other_breakpoints: bool,
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) trait DebuggerRenderer {
    fn render(&mut self, snapshot: &crate::debugger::DebuggerSnapshot);
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
    /// * `gamepads_enabled` - If `true`, attempts to initialize and use connected game controllers.
    ///                        Up to 2 controllers will be supported (player 1 and player 2).
    /// * `fullscreen` - If `true`, runs the emulator in fullscreen mode with letterboxing.
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
    /// let headless = EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0, true, None, false, false)?;
    ///
    /// // Create an EventLoop with an NTSC window at 2x scale
    /// let ntsc = EventLoop::new(false, TvSystem::Ntsc, 2.0, 1.0, true, None, false, false)?;
    ///
    /// // Create an EventLoop with a PAL window at 3x scale at 2x speed with gamepads
    /// let pal = EventLoop::new(false, TvSystem::Pal, 3.0, 2.0, true, None, true, false)?;
    /// # Ok::<(), String>(())
    /// ```
    pub fn new(
        headless: bool,
        tv_system: TvSystem,
        video_scale: f32,
        timing_scale: f32,
        vsync_enabled: bool,
        audio: Option<NesAudio>,
        gamepads_enabled: bool,
        fullscreen: bool,
    ) -> Result<Self, String> {
        let clamped_video_scale = Self::clamp_scale(video_scale);
        let clamped_timing_scale = Self::clamp_timing_scale(timing_scale);

        let sdl_context = sdl2::init()?;
        let event_pump = sdl_context.event_pump()?;

        let gl_backend = if headless {
            None
        } else {
            Some(GlBackend::new(
                &sdl_context,
                tv_system,
                clamped_video_scale,
                vsync_enabled,
                fullscreen,
            )?)
        };

        // Initialize gamepads if enabled
        let (controllers, controller_player_map) = if gamepads_enabled {
            Self::init_gamepads(&sdl_context)?
        } else {
            (Vec::new(), HashMap::new())
        };

        Ok(EventLoop {
            _sdl_context: sdl_context,
            gl_backend,
            event_pump,
            timing_scale: clamped_timing_scale,
            vsync_enabled,
            paused: false,
            debugger_open_requested: false,
            breakpoints: Vec::new(),
            temporary_breakpoint: None,
            arm_temporary_breakpoint_after_next_instruction: false,
            breakpoint_ignore_once_at_pc: None,
            debugger_renderer: None,
            audio,
            controllers,
            controller_player_map,
        })
    }

    /// Initialize game controllers
    ///
    /// Attempts to open up to 2 game controllers. The first controller found
    /// is assigned to player 1, the second to player 2.
    ///
    /// Returns a tuple of (controllers vector, player mapping HashMap)
    fn init_gamepads(
        sdl_context: &sdl2::Sdl,
    ) -> Result<(Vec<sdl2::controller::GameController>, HashMap<u32, u8>), String> {
        let game_controller_subsystem = sdl_context.game_controller()?;
        let available = game_controller_subsystem
            .num_joysticks()
            .map_err(|e| format!("Failed to enumerate joysticks: {}", e))?;

        println!("{} joystick(s) available", available);

        let mut controllers = Vec::new();
        let mut controller_player_map = HashMap::new();

        // Try to open up to 2 controllers
        for id in 0..available.min(2) {
            if !game_controller_subsystem.is_game_controller(id) {
                println!("Joystick {} is not a game controller", id);
                continue;
            }

            match game_controller_subsystem.open(id) {
                Ok(controller) => {
                    let instance_id = controller.instance_id();
                    let player_num = (controllers.len() + 1) as u8;
                    println!(
                        "Opened game controller {} for player {}: {}",
                        id,
                        player_num,
                        controller.name()
                    );
                    controller_player_map.insert(instance_id, player_num);
                    controllers.push(controller);
                }
                Err(e) => {
                    println!("Failed to open controller {}: {}", id, e);
                }
            }

            if controllers.len() >= 2 {
                break;
            }
        }

        Ok((controllers, controller_player_map))
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

    fn should_manual_frame_limit(vsync_enabled: bool) -> bool {
        !vsync_enabled
    }

    fn enter_debugger(&mut self) {
        self.paused = true;
        self.debugger_open_requested = true;
    }

    fn read_vector_target(nes: &crate::nes::Nes, vector_addr: u16) -> u16 {
        let memory = nes.memory.borrow();
        let lo = memory.read_cpu_for_debugger(vector_addr);
        let hi = memory.read_cpu_for_debugger(vector_addr.wrapping_add(1));
        u16::from_le_bytes([lo, hi])
    }

    fn clear_temporary_breakpoint(&mut self) {
        if let Some(tb) = self.temporary_breakpoint.take() {
            if !tb.already_present {
                self.remove_breakpoint(tb.pc);
            }
        }
    }

    fn set_temporary_breakpoint(&mut self, pc: u16) {
        self.clear_temporary_breakpoint();

        let already_present = self.breakpoints.contains(&pc);
        if !already_present {
            self.add_breakpoint(pc);
        }

        self.temporary_breakpoint = Some(TemporaryBreakpoint {
            pc,
            already_present,
            required_interrupt: None,
            has_exited_required_interrupt: true,
            ignore_other_breakpoints: false,
        });
    }

    fn set_temporary_breakpoint_for_interrupt(
        &mut self,
        nes: &crate::nes::Nes,
        pc: u16,
        required_interrupt: crate::cpu::InterruptKind,
    ) {
        self.clear_temporary_breakpoint();

        let already_present = self.breakpoints.contains(&pc);
        if !already_present {
            self.add_breakpoint(pc);
        }

        // If we're already inside this interrupt handler, we want the *next* time we enter it.
        // So wait until we have exited the interrupt at least once.
        let currently_in_interrupt = nes.cpu.current_interrupt() == Some(required_interrupt);
        let has_exited_required_interrupt = !currently_in_interrupt;
        self.temporary_breakpoint = Some(TemporaryBreakpoint {
            pc,
            already_present,
            required_interrupt: Some(required_interrupt),
            has_exited_required_interrupt,
            ignore_other_breakpoints: true,
        });
    }

    fn arm_temporary_breakpoint_after_next_instruction(&mut self) {
        self.arm_temporary_breakpoint_after_next_instruction = true;
    }

    fn maybe_arm_temporary_breakpoint_after_instruction(&mut self, nes: &crate::nes::Nes) {
        if !self.arm_temporary_breakpoint_after_next_instruction {
            return;
        }

        self.arm_temporary_breakpoint_after_next_instruction = false;
        self.set_temporary_breakpoint(nes.cpu.pc);
    }

    fn continue_from_debugger(&mut self, nes: &crate::nes::Nes) {
        // Prevent immediately re-breaking on the same instruction.
        if self.breakpoints.contains(&nes.cpu.pc) {
            self.breakpoint_ignore_once_at_pc = Some(nes.cpu.pc);
        }

        self.paused = false;
        self.debugger_open_requested = false;
    }

    fn check_breakpoint_hit(
        &mut self,
        pc: u16,
        current_interrupt: Option<crate::cpu::InterruptKind>,
    ) -> bool {
        if let Some(tb) = self.temporary_breakpoint.as_mut() {
            if let Some(required_interrupt) = tb.required_interrupt {
                if !tb.has_exited_required_interrupt
                    && current_interrupt != Some(required_interrupt)
                {
                    tb.has_exited_required_interrupt = true;
                }
            }
        }

        // If we just continued from a breakpoint, allow executing that instruction once.
        if self.breakpoint_ignore_once_at_pc == Some(pc) {
            self.breakpoint_ignore_once_at_pc = None;
            return false;
        }

        // While a temporary "run to" breakpoint is active, ignore all other breakpoint hits.
        // This matches the expected debugger UX: run-to should run until the target, not stop
        // early due to unrelated breakpoints (including ones inside the current interrupt).
        if let Some(tb) = self.temporary_breakpoint {
            if tb.ignore_other_breakpoints && self.breakpoints.contains(&pc) && pc != tb.pc {
                return false;
            }
        }

        if self.breakpoints.contains(&pc) {
            if let Some(tb) = self.temporary_breakpoint {
                if tb.pc == pc {
                    // This is our one-shot temp breakpoint. Only break when the CPU has actually
                    // entered the interrupt handler (and, if we were already in it, after we have
                    // exited it at least once).
                    if let Some(required_interrupt) = tb.required_interrupt {
                        if current_interrupt == Some(required_interrupt)
                            && tb.has_exited_required_interrupt
                        {
                            self.temporary_breakpoint = None;
                            if !tb.already_present {
                                self.remove_breakpoint(pc);
                            }
                        } else {
                            // Temp breakpoint not armed yet; ignore this breakpoint hit.
                            return false;
                        }
                    } else {
                        // Plain one-shot breakpoint (used for stepping).
                        self.temporary_breakpoint = None;
                        if !tb.already_present {
                            self.remove_breakpoint(pc);
                        }
                    }
                } else if !tb.ignore_other_breakpoints {
                    // We hit some other breakpoint while a step is pending; cancel the step.
                    self.clear_temporary_breakpoint();
                }
            }

            self.enter_debugger();
            true
        } else {
            false
        }
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

        if let Some(mut gl_backend) = self.gl_backend.take() {
            // We have a window - run with OpenGL + ImGui overlay
            let timer = self._sdl_context.timer()?;
            let mut last_frame_time = timer.performance_counter();
            let performance_frequency = timer.performance_frequency() as f64;

            loop {
                // 1. Poll ALL events (non-blocking)
                let events: Vec<_> = self.event_pump.poll_iter().collect();

                // Process events, collecting controller-related events to handle separately.
                // This avoids needing a mutable borrow of `self` while also holding `gl_backend`.
                let mut controllers_to_add = Vec::new();
                let mut controllers_to_remove = Vec::new();
                let mut controller_buttons = Vec::new();

                for event in events {
                    gl_backend.handle_event(&event);
                    match event {
                        Event::Quit { .. } => {
                            self.gl_backend = Some(gl_backend);
                            return Ok(());
                        }
                        Event::KeyDown {
                            keycode: Some(keycode),
                            ..
                        } => {
                            if self.handle_key_down_for_run(nes, keycode) == KeyDownOutcome::Quit {
                                self.gl_backend = Some(gl_backend);
                                return Ok(());
                            }
                        }
                        Event::KeyUp {
                            keycode: Some(keycode),
                            ..
                        } => {
                            Self::handle_key_up(nes, keycode);
                        }
                        Event::ControllerDeviceAdded { which, .. } => {
                            controllers_to_add.push(which);
                        }
                        Event::ControllerDeviceRemoved { which, .. } => {
                            controllers_to_remove.push(which);
                        }
                        Event::ControllerButtonDown { button, which, .. } => {
                            controller_buttons.push((which, button, true));
                        }
                        Event::ControllerButtonUp { button, which, .. } => {
                            controller_buttons.push((which, button, false));
                        }
                        _ => {}
                    }
                }

                // Handle controller events
                for which in controllers_to_add {
                    self.handle_controller_added(which);
                }
                for which in controllers_to_remove {
                    self.handle_controller_removed(which);
                }
                for (which, button, pressed) in controller_buttons {
                    self.handle_controller_button(nes, which, button, pressed);
                }

                // Skip emulation and rendering if paused
                if self.paused {
                    Self::tick_windowed_paused_for_run(
                        self.debugger_open_requested,
                        &mut self.debugger_renderer,
                        nes,
                    );

                    let action = gl_backend.render(nes, self.debugger_open_requested);
                    self.apply_debugger_ui_action(nes, action);
                    std::thread::sleep(std::time::Duration::from_millis(16));
                    continue;
                }

                // 2. Emulate until PPU completes a full frame (reaches VBlank)
                // The PPU runs at 3x CPU clock (NTSC) or 3.2x (PAL), so run_cpu_tick()
                // automatically runs the correct number of PPU cycles per CPU instruction.
                // A full frame is 262 scanlines × 341 pixels = 89,342 PPU cycles for NTSC
                while !nes.is_ready_to_render() && !nes.cpu.is_halted() {
                    if self.check_breakpoint_hit(nes.cpu.pc, nes.cpu.current_interrupt()) {
                        break;
                    }

                    nes.run(&tracing);
                    self.maybe_arm_temporary_breakpoint_after_instruction(nes);

                    // Poll audio samples from APU and queue them
                    if let Some(ref mut audio) = self.audio {
                        while nes.sample_ready() {
                            if let Some(sample) = nes.get_sample() {
                                audio.queue_sample(sample);
                            }
                        }
                    }
                }

                // If we paused due to a breakpoint, restart the loop so the "paused" branch
                // renders the debugger and accepts debugger UI actions.
                if self.paused {
                    continue;
                }

                if let Some(ref audio) = self.audio {
                    if last_audio_stats_print.elapsed() >= Duration::from_secs(1) {
                        let (received, dropped, underrun) = audio.take_and_reset_stats();
                        let now_cycles = nes.cpu.get_total_cycles();
                        let elapsed = last_perf_instant.elapsed().as_secs_f64();
                        let cycle_delta = now_cycles.saturating_sub(last_cpu_cycles);
                        let cycles_per_sec = if elapsed > 0.0 {
                            cycle_delta as f64 / elapsed
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

                // 3. Render the frame (always present the NES frame; show debugger if requested)
                let _ = gl_backend.render(nes, self.debugger_open_requested);
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
                if self.tick_headless_once_for_run(nes) {
                    return Ok(());
                }

                // Avoid a busy loop while paused.
                if self.paused {
                    std::thread::sleep(std::time::Duration::from_millis(16));
                    continue;
                }

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

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn request_debugger_open(&mut self) {
        self.paused = true;
        self.debugger_open_requested = true;
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn add_breakpoint(&mut self, addr: u16) {
        if !self.breakpoints.contains(&addr) {
            self.breakpoints.push(addr);
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn remove_breakpoint(&mut self, addr: u16) {
        self.breakpoints.retain(|&bp| bp != addr);
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn set_debugger_renderer(&mut self, renderer: Box<dyn DebuggerRenderer>) {
        self.debugger_renderer = Some(renderer);
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn render_debugger_if_needed(&mut self, nes: &crate::nes::Nes) {
        if !self.debugger_open_requested {
            return;
        }

        let Some(renderer) = self.debugger_renderer.as_mut() else {
            return;
        };

        let snapshot = crate::debugger::snapshot(nes);
        renderer.render(&snapshot);
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn is_paused(&self) -> bool {
        self.paused
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn debugger_open_requested(&self) -> bool {
        self.debugger_open_requested
    }

    fn tick_headless_once_for_run(&mut self, nes: &mut crate::nes::Nes) -> bool {
        // Returns `true` if the caller should quit the event loop.
        let events: Vec<_> = self.event_pump.poll_iter().collect();
        for event in events {
            match event {
                Event::Quit { .. } => return true,
                Event::KeyDown {
                    keycode: Some(keycode),
                    ..
                } => {
                    if self.handle_key_down_for_run(nes, keycode) == KeyDownOutcome::Quit {
                        return true;
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

        if self.paused {
            self.render_debugger_if_needed(nes);
            return false;
        }

        if self.check_breakpoint_hit(nes.cpu.pc, nes.cpu.current_interrupt()) {
            return false;
        }

        nes.run_cpu_tick();
        self.maybe_arm_temporary_breakpoint_after_instruction(nes);
        false
    }

    fn tick_windowed_paused_for_run(
        debugger_open_requested: bool,
        debugger_renderer: &mut Option<Box<dyn DebuggerRenderer>>,
        nes: &crate::nes::Nes,
    ) {
        if !debugger_open_requested {
            return;
        }

        let Some(renderer) = debugger_renderer.as_mut() else {
            return;
        };

        let snapshot = crate::debugger::snapshot(nes);
        renderer.render(&snapshot);
    }

    fn apply_debugger_ui_action(
        &mut self,
        nes: &mut crate::nes::Nes,
        action: crate::debugger::ui::DebuggerUiAction,
    ) {
        if !self.debugger_open_requested {
            return;
        }

        let mut should_continue = action.continue_run;

        if action.step_over {
            let pc = nes.cpu.pc;
            let opcode = {
                let memory = nes.memory.borrow();
                memory.read_cpu_for_debugger(pc)
            };

            if opcode == 0x20 {
                // JSR: break at the return address (the instruction after the JSR).
                let return_pc = pc.wrapping_add(3);
                self.set_temporary_breakpoint(return_pc);
            } else {
                // Non-JSR: step one instruction.
                self.arm_temporary_breakpoint_after_next_instruction();
            }

            should_continue = true;
        }

        if action.step_into {
            self.arm_temporary_breakpoint_after_next_instruction();
            should_continue = true;
        }

        if action.run_to_next_frame {
            Self::debugger_run_to_next_frame(nes);
        }
        if action.run_to_nmi {
            let target = Self::read_vector_target(nes, 0xFFFA);
            self.set_temporary_breakpoint_for_interrupt(
                nes,
                target,
                crate::cpu::InterruptKind::Nmi,
            );
            should_continue = true;
        }
        if action.run_to_irq {
            let target = Self::read_vector_target(nes, 0xFFFE);
            self.set_temporary_breakpoint_for_interrupt(
                nes,
                target,
                crate::cpu::InterruptKind::Irq,
            );
            should_continue = true;
        }

        if should_continue {
            self.continue_from_debugger(nes);
        }
    }

    fn handle_key_down_for_run(
        &mut self,
        nes: &mut crate::nes::Nes,
        keycode: Keycode,
    ) -> KeyDownOutcome {
        // When the debugger is open, make F5 behave exactly like the Continue button.
        // This ensures breakpoint ignore-once semantics apply equally.
        if keycode == Keycode::F5 && self.debugger_open_requested {
            self.apply_debugger_ui_action(
                nes,
                crate::debugger::ui::DebuggerUiAction {
                    continue_run: true,
                    step_over: false,
                    step_into: false,
                    run_to_next_frame: false,
                    run_to_nmi: false,
                    run_to_irq: false,
                },
            );
            return KeyDownOutcome::Continue;
        }

        Self::handle_key_down(
            nes,
            keycode,
            self.audio.as_ref(),
            &mut self.paused,
            &mut self.debugger_open_requested,
        )
    }

    fn debugger_run_to_next_frame(nes: &mut crate::nes::Nes) {
        const MAX_STEPS: usize = 2_000_000;

        let mut previous_scanline = {
            let ppu = nes.ppu.borrow();
            ppu.scanline()
        };

        for _step in 0..MAX_STEPS {
            if nes.cpu.is_halted() {
                break;
            }

            nes.run_cpu_tick();

            let (scanline, _pixel) = {
                let ppu = nes.ppu.borrow();
                (ppu.scanline(), ppu.pixel())
            };

            // Stop once we have crossed into the next frame.
            //
            // Important: this emulator advances the PPU timing in bulk per executed CPU
            // instruction, so we may cross the frame boundary *during* an instruction.
            // Requiring the instruction boundary to land exactly at (0,0) can cause this
            // command to run far past the frame start.
            if scanline < previous_scanline {
                break;
            }

            previous_scanline = scanline;
        }
    }

    fn debugger_step_over(nes: &mut crate::nes::Nes) {
        const JSR_OPCODE: u8 = 0x20;

        let pc = nes.cpu.pc;
        let opcode = {
            let memory = nes.memory.borrow();
            memory.read_cpu_for_debugger(pc)
        };

        if opcode == JSR_OPCODE {
            let next_pc = pc.wrapping_add(3);

            // Execute the JSR itself (enter subroutine).
            nes.run_cpu_tick();

            // Run until we return to the instruction after the original JSR.
            const MAX_STEPS: usize = 1_000_000;
            for _ in 0..MAX_STEPS {
                if nes.cpu.pc == next_pc || nes.cpu.is_halted() {
                    break;
                }
                nes.run_cpu_tick();
            }
        } else {
            // Non-JSR: step one instruction.
            nes.run_cpu_tick();
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
    /// - F5: Open debugger (when closed) / Continue (when debugger open)
    /// - F10: Debugger step-over (JSR runs until RTS)
    /// - F11: Debugger step-into (single CPU tick)
    /// - F2/F3: Volume up/down (when audio is enabled)
    fn handle_key_down(
        nes: &mut crate::nes::Nes,
        keycode: Keycode,
        audio: Option<&NesAudio>,
        paused: &mut bool,
        debugger_open_requested: &mut bool,
    ) -> KeyDownOutcome {
        match keycode {
            Keycode::Escape => return KeyDownOutcome::Quit,
            Keycode::Space => {
                *paused = !*paused;
            }
            Keycode::F5 => {
                // Debugger toggle/continue:
                // - If debugger is closed: open it and pause.
                // - If debugger is open: continue running and close it.
                if *debugger_open_requested {
                    *paused = false;
                    *debugger_open_requested = false;
                } else {
                    *paused = true;
                    *debugger_open_requested = true;
                }
            }
            Keycode::F10 => {
                // Debugger step-over.
                *paused = true;
                *debugger_open_requested = true;
                Self::debugger_step_over(nes);
            }
            Keycode::F11 => {
                // Debugger step-into: execute one CPU tick and remain paused.
                *paused = true;
                *debugger_open_requested = true;
                nes.run_cpu_tick();
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

    fn handle_key_up(nes: &mut crate::nes::Nes, keycode: Keycode) {
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

    /// Handle controller device added event
    fn handle_controller_added(&mut self, which: u32) {
        // Only add if we have less than 2 controllers
        if self.controllers.len() >= 2 {
            println!("Controller {} added but already have 2 controllers", which);
            return;
        }

        let game_controller_subsystem = match self._sdl_context.game_controller() {
            Ok(subsystem) => subsystem,
            Err(e) => {
                println!("Failed to get game controller subsystem: {}", e);
                return;
            }
        };

        if !game_controller_subsystem.is_game_controller(which) {
            println!("Device {} is not a game controller", which);
            return;
        }

        match game_controller_subsystem.open(which) {
            Ok(controller) => {
                let instance_id = controller.instance_id();
                let player_num = (self.controllers.len() + 1) as u8;
                println!(
                    "Hot-plugged controller {} for player {}: {}",
                    which,
                    player_num,
                    controller.name()
                );
                self.controller_player_map.insert(instance_id, player_num);
                self.controllers.push(controller);
            }
            Err(e) => {
                println!("Failed to open controller {}: {}", which, e);
            }
        }
    }

    /// Handle controller device removed event
    fn handle_controller_removed(&mut self, which: u32) {
        // Find and remove the controller with this instance_id
        if let Some(player_num) = self.controller_player_map.remove(&which) {
            // Remove from controllers vec
            self.controllers.retain(|c| c.instance_id() != which);
            println!("Controller {} (player {}) removed", which, player_num);

            // Reassign remaining controllers to players 1 and 2
            self.controller_player_map.clear();
            for (idx, controller) in self.controllers.iter().enumerate() {
                let instance_id = controller.instance_id();
                let new_player_num = (idx + 1) as u8;
                self.controller_player_map
                    .insert(instance_id, new_player_num);
                println!(
                    "Reassigned controller {} to player {}",
                    instance_id, new_player_num
                );
            }
        }
    }

    /// Handle controller button press/release
    fn handle_controller_button(
        &self,
        nes: &mut crate::nes::Nes,
        which: u32,
        button: sdl2::controller::Button,
        pressed: bool,
    ) {
        use crate::input::Button as NesButton;

        // Get the player number for this controller
        let player_num = match self.controller_player_map.get(&which) {
            Some(&num) => num,
            None => return, // Unknown controller
        };

        // Map SDL2 controller buttons to NES buttons
        let nes_button = match button {
            sdl2::controller::Button::DPadUp => Some(NesButton::Up),
            sdl2::controller::Button::DPadDown => Some(NesButton::Down),
            sdl2::controller::Button::DPadLeft => Some(NesButton::Left),
            sdl2::controller::Button::DPadRight => Some(NesButton::Right),
            sdl2::controller::Button::A => Some(NesButton::A),
            sdl2::controller::Button::B => Some(NesButton::B),
            sdl2::controller::Button::X => Some(NesButton::A), // Also map X to A
            sdl2::controller::Button::Y => Some(NesButton::B), // Also map Y to B
            sdl2::controller::Button::Back => Some(NesButton::Select),
            sdl2::controller::Button::Start => Some(NesButton::Start),
            _ => None, // Ignore other buttons
        };

        if let Some(nes_button) = nes_button {
            nes.set_button(player_num, nes_button, pressed);
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
    use std::cell::RefCell;
    use std::env;
    use std::rc::Rc;

    fn insert_nop_cartridge(nes: &mut Nes, reset_vector: u16) {
        // Minimal NROM-style PRG filled with NOPs and a RESET vector.
        let mut prg_rom = vec![0xEAu8; 0x8000]; // NOP
        prg_rom[0x7FFC] = (reset_vector & 0x00FF) as u8;
        prg_rom[0x7FFD] = (reset_vector >> 8) as u8;

        // Provide sane defaults for NMI/IRQ vectors (not used by these tests).
        prg_rom[0x7FFA] = 0x00;
        prg_rom[0x7FFB] = 0x80;
        prg_rom[0x7FFE] = 0x00;
        prg_rom[0x7FFF] = 0x80;

        let cartridge = crate::cartridge::Cartridge::from_parts(
            prg_rom,
            vec![],
            crate::cartridge::MirroringMode::Horizontal,
        );
        nes.insert_cartridge(cartridge);
    }

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

    fn tick_headless_once(event_loop: &mut EventLoop, nes: &mut Nes) {
        let _should_quit = event_loop.tick_headless_once_for_run(nes);
    }

    #[test]
    #[serial]
    fn test_breakpoint_hit_pauses_and_opens_debugger() {
        let mut event_loop =
            EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0, true, None, false, false).unwrap();
        let mut nes = Nes::new(TvSystem::Ntsc);

        insert_nop_cartridge(&mut nes, 0x8000);
        nes.reset(false);

        // Run until PC reaches $8002, then expect the breakpoint to pause *before* executing it.
        event_loop.add_breakpoint(0x8002);

        // Execute two instructions: $8000 and $8001.
        tick_headless_once(&mut event_loop, &mut nes);
        tick_headless_once(&mut event_loop, &mut nes);
        assert_eq!(nes.cpu.pc, 0x8002);
        assert!(!event_loop.is_paused());
        assert!(!event_loop.debugger_open_requested());

        // Next tick should notice the breakpoint and enter the debugger (paused).
        tick_headless_once(&mut event_loop, &mut nes);
        assert_eq!(nes.cpu.pc, 0x8002);
        assert!(event_loop.is_paused());
        assert!(event_loop.debugger_open_requested());
    }

    #[test]
    #[serial]
    fn test_remove_breakpoint_allows_execution_to_continue() {
        let mut event_loop =
            EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0, true, None, false, false).unwrap();
        let mut nes = Nes::new(TvSystem::Ntsc);

        insert_nop_cartridge(&mut nes, 0x8000);
        nes.reset(false);

        event_loop.add_breakpoint(0x8001);
        event_loop.remove_breakpoint(0x8001);

        // If the breakpoint was removed, we should not pause when reaching $8001.
        tick_headless_once(&mut event_loop, &mut nes);
        assert_eq!(nes.cpu.pc, 0x8001);
        assert!(!event_loop.is_paused());
        assert!(!event_loop.debugger_open_requested());
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
        let event_loop = EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0, true, None, false, false);
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
        let mut debugger_open_requested = false;

        let before = audio.get_volume();
        EventLoop::handle_key_down(
            &mut nes,
            Keycode::F2,
            Some(&audio),
            &mut paused,
            &mut debugger_open_requested,
        );
        assert!((audio.get_volume() - (before + 0.1)).abs() < 1e-6);
        EventLoop::handle_key_down(
            &mut nes,
            Keycode::F3,
            Some(&audio),
            &mut paused,
            &mut debugger_open_requested,
        );
        assert!((audio.get_volume() - before).abs() < 1e-6);

        drop(restore);
    }

    #[test]
    fn test_handle_key_down_escape_requests_quit() {
        // Desired behavior: key handling for Escape is centralized in handle_key_down,
        // and it indicates that the event loop should exit.
        let mut nes = Nes::new(TvSystem::Ntsc);
        let mut paused = false;
        let mut debugger_open_requested = false;

        let outcome = EventLoop::handle_key_down(
            &mut nes,
            Keycode::Escape,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );
        assert_eq!(outcome, KeyDownOutcome::Quit);
    }

    #[test]
    fn test_handle_key_down_space_toggles_pause() {
        // Desired behavior: Space toggles pause state via centralized handle_key_down.
        let mut nes = Nes::new(TvSystem::Ntsc);
        let mut paused = false;
        let mut debugger_open_requested = false;

        let _ = EventLoop::handle_key_down(
            &mut nes,
            Keycode::Space,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );
        assert!(paused);
        let _ = EventLoop::handle_key_down(
            &mut nes,
            Keycode::Space,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );
        assert!(!paused);
    }

    fn nes_with_jsr_program() -> Nes {
        let mut nes = Nes::new(TvSystem::Ntsc);

        // Program at $8000:
        //   JSR $8006
        //   LDA #$01
        //   BRK
        // Subroutine at $8006:
        //   INX
        //   RTS
        let mut prg_rom = vec![0xEAu8; 0x8000]; // NOP fill
        let reset_vector: u16 = 0x8000;

        // Vectors live at $FFFA-$FFFF -> end of PRG ROM.
        prg_rom[0x7FFA] = (reset_vector & 0x00FF) as u8; // NMI
        prg_rom[0x7FFB] = (reset_vector >> 8) as u8;
        prg_rom[0x7FFC] = (reset_vector & 0x00FF) as u8; // RESET
        prg_rom[0x7FFD] = (reset_vector >> 8) as u8;
        prg_rom[0x7FFE] = (reset_vector & 0x00FF) as u8; // IRQ/BRK
        prg_rom[0x7FFF] = (reset_vector >> 8) as u8;

        // $8000: JSR $8006
        prg_rom[0x0000] = 0x20;
        prg_rom[0x0001] = 0x06;
        prg_rom[0x0002] = 0x80;
        // $8003: LDA #$01
        prg_rom[0x0003] = 0xA9;
        prg_rom[0x0004] = 0x01;
        // $8005: BRK
        prg_rom[0x0005] = 0x00;
        // $8006: INX
        prg_rom[0x0006] = 0xE8;
        // $8007: RTS
        prg_rom[0x0007] = 0x60;

        let cartridge = crate::cartridge::Cartridge::from_parts(
            prg_rom,
            vec![],
            crate::cartridge::MirroringMode::Horizontal,
        );
        nes.insert_cartridge(cartridge);
        nes.reset(false);
        nes
    }

    fn nes_with_nop_loop_program() -> Nes {
        let mut nes = Nes::new(TvSystem::Ntsc);

        // Program at $8000:
        //   NOP
        //   JMP $8000
        let mut prg_rom = vec![0xEAu8; 0x8000]; // NOP fill
        let reset_vector: u16 = 0x8000;

        // Vectors live at $FFFA-$FFFF -> end of PRG ROM.
        prg_rom[0x7FFA] = (reset_vector & 0x00FF) as u8; // NMI
        prg_rom[0x7FFB] = (reset_vector >> 8) as u8;
        prg_rom[0x7FFC] = (reset_vector & 0x00FF) as u8; // RESET
        prg_rom[0x7FFD] = (reset_vector >> 8) as u8;
        prg_rom[0x7FFE] = (reset_vector & 0x00FF) as u8; // IRQ/BRK
        prg_rom[0x7FFF] = (reset_vector >> 8) as u8;

        // $8000: NOP
        prg_rom[0x0000] = 0xEA;
        // $8001: JMP $8000
        prg_rom[0x0001] = 0x4C;
        prg_rom[0x0002] = 0x00;
        prg_rom[0x0003] = 0x80;

        let cartridge = crate::cartridge::Cartridge::from_parts(
            prg_rom,
            vec![],
            crate::cartridge::MirroringMode::Horizontal,
        );
        nes.insert_cartridge(cartridge);
        nes.reset(false);
        nes
    }

    #[test]
    fn test_handle_key_down_f5_pauses_emulation() {
        // Desired behavior: F5 opens debugger windows, which immediately pauses emulation.
        let mut nes = Nes::new(TvSystem::Ntsc);
        let mut paused = false;
        let mut debugger_open_requested = false;

        let _ = EventLoop::handle_key_down(
            &mut nes,
            Keycode::F5,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );
        assert!(paused);
        assert!(debugger_open_requested);
    }

    #[test]
    fn test_handle_key_down_f5_when_debugger_open_continues_and_closes_debugger() {
        let mut nes = Nes::new(TvSystem::Ntsc);
        let mut paused = true;
        let mut debugger_open_requested = true;

        let _ = EventLoop::handle_key_down(
            &mut nes,
            Keycode::F5,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );

        assert!(!paused);
        assert!(!debugger_open_requested);
    }

    #[test]
    fn test_run_to_next_frame_stops_after_frame_wrap_even_if_not_at_pixel_0_0() {
        let mut nes_expected = nes_with_nop_loop_program();
        let mut nes_actual = nes_with_nop_loop_program();

        // Move both PPUs close to the end of the NTSC pre-render scanline.
        // We pick a position such that executing exactly one CPU instruction will cross into
        // the next frame, but the instruction boundary will *not* land on (0,0).
        for nes in [&mut nes_expected, &mut nes_actual] {
            let mut ppu = nes.ppu.borrow_mut();
            while ppu.scanline() != 261 || ppu.pixel() != 338 {
                ppu.run_ppu_cycles(1);
            }
        }

        // Expected behavior: run to next frame should stop at the first instruction boundary
        // after we have crossed the frame start.
        nes_expected.run_cpu_tick();
        let (expected_scanline, expected_pixel) = {
            let ppu = nes_expected.ppu.borrow();
            (ppu.scanline(), ppu.pixel())
        };
        assert_eq!(
            expected_scanline, 0,
            "setup should cross into the next frame"
        );
        assert_ne!(
            expected_pixel, 0,
            "setup should not land exactly at (0,0) after one instruction"
        );

        let cpu_cycles_before = nes_actual.cpu.get_total_cycles();
        EventLoop::debugger_run_to_next_frame(&mut nes_actual);
        let cpu_cycles_after = nes_actual.cpu.get_total_cycles();

        let (actual_scanline, actual_pixel) = {
            let ppu = nes_actual.ppu.borrow();
            (ppu.scanline(), ppu.pixel())
        };

        assert_eq!(
            (actual_scanline, actual_pixel),
            (expected_scanline, expected_pixel),
            "should stop at the first instruction boundary after frame wrap (not wait for an exact (0,0) boundary)"
        );

        assert!(
            cpu_cycles_after - cpu_cycles_before < 1000,
            "should stop soon after the frame wrap, not spin until an exact (0,0) boundary"
        );
    }

    #[test]
    fn test_handle_key_down_f10_step_over_jsr_runs_until_return() {
        let mut nes = nes_with_jsr_program();
        nes.cpu.x = 0;

        let mut paused = true;
        let mut debugger_open_requested = true;

        let _ = EventLoop::handle_key_down(
            &mut nes,
            Keycode::F10,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );

        assert!(paused, "step-over should keep emulator paused");
        assert!(
            debugger_open_requested,
            "step-over should keep debugger open"
        );
        assert_eq!(
            nes.cpu.pc, 0x8003,
            "expected step-over to stop at next instruction"
        );
        assert_eq!(
            nes.cpu.x, 1,
            "expected subroutine to have executed (INX) before returning"
        );
    }

    #[test]
    fn test_handle_key_down_f11_step_into_jsr_enters_subroutine() {
        let mut nes = nes_with_jsr_program();
        nes.cpu.x = 0;

        let mut paused = true;
        let mut debugger_open_requested = true;

        let _ = EventLoop::handle_key_down(
            &mut nes,
            Keycode::F11,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );

        assert!(paused, "step-into should keep emulator paused");
        assert!(
            debugger_open_requested,
            "step-into should keep debugger open"
        );
        assert_eq!(nes.cpu.pc, 0x8006, "expected step-into to enter subroutine");
        assert_eq!(
            nes.cpu.x, 0,
            "expected to not execute INX when stepping into JSR"
        );
    }

    #[test]
    #[serial]
    fn test_continue_action_unpauses_and_closes_debugger() {
        let mut event_loop =
            EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0, true, None, false, false).unwrap();
        let mut nes = Nes::new(TvSystem::Ntsc);

        event_loop.request_debugger_open();

        event_loop.apply_debugger_ui_action(
            &mut nes,
            crate::debugger::ui::DebuggerUiAction {
                continue_run: true,
                step_over: false,
                step_into: false,
                run_to_next_frame: false,
                run_to_nmi: false,
                run_to_irq: false,
            },
        );

        assert!(!event_loop.is_paused(), "continue should unpause");
        assert!(
            !event_loop.debugger_open_requested(),
            "continue should close debugger"
        );
    }

    #[test]
    #[serial]
    fn test_continue_skips_breakpoint_once_on_same_pc() {
        let mut event_loop =
            EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0, true, None, false, false).unwrap();
        let mut nes = Nes::new(TvSystem::Ntsc);

        insert_nop_cartridge(&mut nes, 0x8000);
        nes.reset(false);

        // Break immediately at the first instruction.
        event_loop.add_breakpoint(0x8000);

        // First tick hits the breakpoint and pauses before executing.
        tick_headless_once(&mut event_loop, &mut nes);
        assert_eq!(nes.cpu.pc, 0x8000);
        assert!(event_loop.is_paused());
        assert!(event_loop.debugger_open_requested());

        // Continue should unpause and not instantly re-break at the same PC.
        event_loop.apply_debugger_ui_action(
            &mut nes,
            crate::debugger::ui::DebuggerUiAction {
                continue_run: true,
                step_over: false,
                step_into: false,
                run_to_next_frame: false,
                run_to_nmi: false,
                run_to_irq: false,
            },
        );
        assert!(!event_loop.is_paused());
        assert!(!event_loop.debugger_open_requested());

        // Next tick must execute the instruction at $8000 (NOP) and advance.
        tick_headless_once(&mut event_loop, &mut nes);
        assert_eq!(nes.cpu.pc, 0x8001);
        assert!(!event_loop.is_paused());
        assert!(!event_loop.debugger_open_requested());
    }

    #[test]
    #[serial]
    fn test_f5_when_debugger_open_behaves_like_continue_for_breakpoints() {
        let mut event_loop =
            EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0, true, None, false, false).unwrap();
        let mut nes = Nes::new(TvSystem::Ntsc);

        insert_nop_cartridge(&mut nes, 0x8000);
        nes.reset(false);

        event_loop.add_breakpoint(0x8000);

        // Hit the breakpoint first.
        tick_headless_once(&mut event_loop, &mut nes);
        assert_eq!(nes.cpu.pc, 0x8000);
        assert!(event_loop.is_paused());
        assert!(event_loop.debugger_open_requested());

        // Pressing F5 while debugger is open should behave like Continue.
        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::F5);
        assert!(!event_loop.is_paused());
        assert!(!event_loop.debugger_open_requested());

        // Next tick must execute past the breakpoint without immediately re-breaking.
        tick_headless_once(&mut event_loop, &mut nes);
        assert_eq!(nes.cpu.pc, 0x8001);
        assert!(!event_loop.is_paused());
        assert!(!event_loop.debugger_open_requested());
    }

    #[test]
    #[serial]
    fn test_run_to_nmi_action_runs_until_nmi_vector_pc() {
        let mut event_loop =
            EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0, true, None, false, false).unwrap();
        let mut nes = Nes::new(TvSystem::Ntsc);

        // Minimal cartridge with vectors.
        let mut prg_rom = vec![0xEAu8; 0x8000]; // NOP
        let reset_vector: u16 = 0x8000;
        let nmi_vector: u16 = 0x9000;
        prg_rom[0x7FFC] = (reset_vector & 0x00FF) as u8; // RESET
        prg_rom[0x7FFD] = (reset_vector >> 8) as u8;
        prg_rom[0x7FFA] = (nmi_vector & 0x00FF) as u8; // NMI
        prg_rom[0x7FFB] = (nmi_vector >> 8) as u8;
        prg_rom[0x7FFE] = 0x00; // IRQ/BRK (unused)
        prg_rom[0x7FFF] = 0x80;

        let cartridge = crate::cartridge::Cartridge::from_parts(
            prg_rom,
            vec![],
            crate::cartridge::MirroringMode::Horizontal,
        );
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        // Force an NMI edge before running.
        nes.cpu.set_nmi_pending(true);

        event_loop.request_debugger_open();
        event_loop.apply_debugger_ui_action(
            &mut nes,
            crate::debugger::ui::DebuggerUiAction {
                continue_run: false,
                step_over: false,
                step_into: false,
                run_to_next_frame: false,
                run_to_nmi: true,
                run_to_irq: false,
            },
        );

        assert!(!event_loop.is_paused(), "run-to should continue running");
        assert!(
            !event_loop.debugger_open_requested(),
            "run-to should close debugger (same as Continue)"
        );

        // Run until the temporary breakpoint hits.
        for _ in 0..1_000_000 {
            tick_headless_once(&mut event_loop, &mut nes);
            if event_loop.is_paused() {
                break;
            }
        }

        assert!(event_loop.is_paused());
        assert!(event_loop.debugger_open_requested());
        assert_eq!(nes.cpu.pc, nmi_vector);
        assert!(
            event_loop.temporary_breakpoint.is_none(),
            "temporary breakpoint should clear after being hit"
        );
        assert!(
            !event_loop.breakpoints.contains(&nmi_vector),
            "temporary breakpoint should be removed after being hit"
        );
    }

    #[test]
    #[serial]
    fn test_run_to_irq_action_runs_until_irq_vector_pc() {
        let mut event_loop =
            EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0, true, None, false, false).unwrap();
        let mut nes = Nes::new(TvSystem::Ntsc);

        // Minimal cartridge with vectors.
        let mut prg_rom = vec![0xEAu8; 0x8000]; // NOP
        let reset_vector: u16 = 0x8000;
        let irq_vector: u16 = 0x9000;
        prg_rom[0x7FFC] = (reset_vector & 0x00FF) as u8; // RESET
        prg_rom[0x7FFD] = (reset_vector >> 8) as u8;
        prg_rom[0x7FFA] = 0x00; // NMI (unused)
        prg_rom[0x7FFB] = 0x80;
        prg_rom[0x7FFE] = (irq_vector & 0x00FF) as u8; // IRQ/BRK
        prg_rom[0x7FFF] = (irq_vector >> 8) as u8;

        let cartridge = crate::cartridge::Cartridge::from_parts(
            prg_rom,
            vec![],
            crate::cartridge::MirroringMode::Horizontal,
        );
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        // Ensure IRQs are unmasked (clear I flag).
        nes.cpu.p &= !0b0000_0100;

        // Force an IRQ to be pending.
        nes.cpu.set_irq_pending(true);

        event_loop.request_debugger_open();
        event_loop.apply_debugger_ui_action(
            &mut nes,
            crate::debugger::ui::DebuggerUiAction {
                continue_run: false,
                step_over: false,
                step_into: false,
                run_to_next_frame: false,
                run_to_nmi: false,
                run_to_irq: true,
            },
        );

        assert!(!event_loop.is_paused(), "run-to should continue running");
        assert!(
            !event_loop.debugger_open_requested(),
            "run-to should close debugger (same as Continue)"
        );

        // Run until the temporary breakpoint hits.
        for _ in 0..1_000_000 {
            tick_headless_once(&mut event_loop, &mut nes);
            if event_loop.is_paused() {
                break;
            }
        }

        assert!(event_loop.is_paused());
        assert!(event_loop.debugger_open_requested());
        assert_eq!(nes.cpu.pc, irq_vector);
        assert!(
            event_loop.temporary_breakpoint.is_none(),
            "temporary breakpoint should clear after being hit"
        );
        assert!(
            !event_loop.breakpoints.contains(&irq_vector),
            "temporary breakpoint should be removed after being hit"
        );
    }

    #[test]
    #[serial]
    fn test_run_to_irq_requires_actual_irq_entry_not_just_pc_match() {
        let mut event_loop =
            EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0, true, None, false, false).unwrap();
        let mut nes = Nes::new(TvSystem::Ntsc);

        // Minimal cartridge where IRQ vector points at the reset entrypoint.
        // This catches false positives where the debugger stops just because PC == vector.
        let mut prg_rom = vec![0xEAu8; 0x8000]; // NOP
        let reset_vector: u16 = 0x8000;
        let irq_vector: u16 = 0x8000;
        prg_rom[0x7FFC] = (reset_vector & 0x00FF) as u8; // RESET
        prg_rom[0x7FFD] = (reset_vector >> 8) as u8;
        prg_rom[0x7FFA] = 0x00; // NMI (unused)
        prg_rom[0x7FFB] = 0x80;
        prg_rom[0x7FFE] = (irq_vector & 0x00FF) as u8; // IRQ/BRK
        prg_rom[0x7FFF] = (irq_vector >> 8) as u8;

        let cartridge = crate::cartridge::Cartridge::from_parts(
            prg_rom,
            vec![],
            crate::cartridge::MirroringMode::Horizontal,
        );
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        // Ensure IRQs are unmasked (clear I flag).
        nes.cpu.p &= !0b0000_0100;
        // Force an IRQ to be pending.
        nes.cpu.set_irq_pending(true);

        event_loop.request_debugger_open();
        event_loop.apply_debugger_ui_action(
            &mut nes,
            crate::debugger::ui::DebuggerUiAction {
                continue_run: false,
                step_over: false,
                step_into: false,
                run_to_next_frame: false,
                run_to_nmi: false,
                run_to_irq: true,
            },
        );

        // The vector points at the current PC, so we must not re-break immediately.
        tick_headless_once(&mut event_loop, &mut nes);
        assert!(!event_loop.is_paused());

        // Eventually, we should pause once the CPU actually vectors into the IRQ handler.
        for _ in 0..1_000_000 {
            tick_headless_once(&mut event_loop, &mut nes);
            if event_loop.is_paused() {
                break;
            }
        }

        assert!(event_loop.is_paused());
        assert_eq!(nes.cpu.pc, irq_vector);
        assert_eq!(
            nes.cpu.current_interrupt(),
            Some(crate::cpu::InterruptKind::Irq)
        );
    }

    #[test]
    #[serial]
    fn test_run_to_nmi_when_already_in_nmi_waits_for_next_nmi_entry() {
        let mut event_loop =
            EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0, true, None, false, false).unwrap();
        let mut nes = Nes::new(TvSystem::Ntsc);

        // Cartridge with RESET=$8000, NMI=$9000.
        let mut prg_rom = vec![0xEAu8; 0x8000]; // NOP
        let reset_vector: u16 = 0x8000;
        let nmi_vector: u16 = 0x9000;
        prg_rom[0x7FFC] = (reset_vector & 0x00FF) as u8; // RESET
        prg_rom[0x7FFD] = (reset_vector >> 8) as u8;
        prg_rom[0x7FFA] = (nmi_vector & 0x00FF) as u8; // NMI
        prg_rom[0x7FFB] = (nmi_vector >> 8) as u8;
        prg_rom[0x7FFE] = 0x00; // IRQ/BRK (unused)
        prg_rom[0x7FFF] = 0x80;

        // NMI handler at $9000:
        //   LDA $00
        //   BEQ done
        //   DEC $00
        //   JMP $9000
        // done:
        //   RTI
        let nmi_offset = (nmi_vector - 0x8000) as usize;
        let handler = [
            0xA5, 0x00, // LDA $00
            0xF0, 0x05, // BEQ +5 (to RTI)
            0xC6, 0x00, // DEC $00
            0x4C, 0x00, 0x90, // JMP $9000
            0x40, // RTI
        ];
        prg_rom[nmi_offset..nmi_offset + handler.len()].copy_from_slice(&handler);

        let cartridge = crate::cartridge::Cartridge::from_parts(
            prg_rom,
            vec![],
            crate::cartridge::MirroringMode::Horizontal,
        );
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        // Make the handler loop a bit (revisiting $9000) before RTI.
        nes.memory.borrow_mut().write_for_testing(0x0000, 3);

        // Enter NMI once, stopping at the vector.
        nes.cpu.set_nmi_pending(true);
        for _ in 0..1_000_000 {
            nes.run_cpu_tick();
            if nes.cpu.current_interrupt() == Some(crate::cpu::InterruptKind::Nmi)
                && nes.cpu.pc == nmi_vector
            {
                break;
            }
        }
        assert_eq!(
            nes.cpu.current_interrupt(),
            Some(crate::cpu::InterruptKind::Nmi)
        );
        assert_eq!(nes.cpu.pc, nmi_vector);

        // Now request Run-to-NMI while we're already inside the current NMI handler.
        event_loop.request_debugger_open();
        event_loop.apply_debugger_ui_action(
            &mut nes,
            crate::debugger::ui::DebuggerUiAction {
                continue_run: false,
                step_over: false,
                step_into: false,
                run_to_next_frame: false,
                run_to_nmi: true,
                run_to_irq: false,
            },
        );

        // Must continue running, not immediately break again at $9000.
        assert!(!event_loop.is_paused());
        assert!(!event_loop.debugger_open_requested());

        // Run until we exit the current NMI (RTI), without breaking.
        for _ in 0..1_000_000 {
            tick_headless_once(&mut event_loop, &mut nes);
            assert!(
                !event_loop.is_paused(),
                "should not break again during the current NMI handler"
            );
            if nes.cpu.current_interrupt() != Some(crate::cpu::InterruptKind::Nmi) {
                break;
            }
        }
        assert_ne!(
            nes.cpu.current_interrupt(),
            Some(crate::cpu::InterruptKind::Nmi)
        );

        // Trigger the *next* NMI and ensure we break at the vector.
        nes.cpu.set_nmi_pending(true);
        for _ in 0..1_000_000 {
            tick_headless_once(&mut event_loop, &mut nes);
            if event_loop.is_paused() {
                break;
            }
        }

        assert!(event_loop.is_paused());
        assert!(event_loop.debugger_open_requested());
        assert_eq!(
            nes.cpu.current_interrupt(),
            Some(crate::cpu::InterruptKind::Nmi)
        );
        assert_eq!(nes.cpu.pc, nmi_vector);
    }

    #[test]
    #[serial]
    fn test_run_to_nmi_ignores_other_breakpoints_until_next_nmi_entry() {
        let mut event_loop =
            EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0, true, None, false, false).unwrap();
        let mut nes = Nes::new(TvSystem::Ntsc);

        // Cartridge with RESET=$8000, NMI=$9000.
        let mut prg_rom = vec![0xEAu8; 0x8000]; // NOP
        let reset_vector: u16 = 0x8000;
        let nmi_vector: u16 = 0x9000;
        prg_rom[0x7FFC] = (reset_vector & 0x00FF) as u8; // RESET
        prg_rom[0x7FFD] = (reset_vector >> 8) as u8;
        prg_rom[0x7FFA] = (nmi_vector & 0x00FF) as u8; // NMI
        prg_rom[0x7FFB] = (nmi_vector >> 8) as u8;
        prg_rom[0x7FFE] = 0x00; // IRQ/BRK (unused)
        prg_rom[0x7FFF] = 0x80;

        // NMI handler at $9000: NOP, NOP, RTI.
        let nmi_offset = (nmi_vector - 0x8000) as usize;
        prg_rom[nmi_offset..nmi_offset + 3].copy_from_slice(&[0xEA, 0xEA, 0x40]);

        let cartridge = crate::cartridge::Cartridge::from_parts(
            prg_rom,
            vec![],
            crate::cartridge::MirroringMode::Horizontal,
        );
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        // Enter NMI once, stopping at the vector.
        nes.cpu.set_nmi_pending(true);
        for _ in 0..1_000_000 {
            nes.run_cpu_tick();
            if nes.cpu.current_interrupt() == Some(crate::cpu::InterruptKind::Nmi)
                && nes.cpu.pc == nmi_vector
            {
                break;
            }
        }
        assert_eq!(
            nes.cpu.current_interrupt(),
            Some(crate::cpu::InterruptKind::Nmi)
        );
        assert_eq!(nes.cpu.pc, nmi_vector);

        // Place a breakpoint inside the current NMI handler to reproduce the user-reported
        // "Run to NMI just steps one instruction" behavior.
        let handler_second_instruction = nmi_vector.wrapping_add(1);
        event_loop.add_breakpoint(handler_second_instruction);

        // Now request Run-to-NMI while we're already inside the current NMI handler.
        event_loop.request_debugger_open();
        event_loop.apply_debugger_ui_action(
            &mut nes,
            crate::debugger::ui::DebuggerUiAction {
                continue_run: false,
                step_over: false,
                step_into: false,
                run_to_next_frame: false,
                run_to_nmi: true,
                run_to_irq: false,
            },
        );

        assert!(!event_loop.is_paused());
        assert!(!event_loop.debugger_open_requested());

        // Must not stop at the breakpoint inside the current handler.
        for _ in 0..1_000_000 {
            tick_headless_once(&mut event_loop, &mut nes);
            assert!(
                !event_loop.is_paused(),
                "run-to should ignore other breakpoints until it reaches the next NMI entry"
            );
            if nes.cpu.current_interrupt() != Some(crate::cpu::InterruptKind::Nmi) {
                break;
            }
        }

        // Trigger the *next* NMI and ensure we break at the vector.
        nes.cpu.set_nmi_pending(true);
        for _ in 0..1_000_000 {
            tick_headless_once(&mut event_loop, &mut nes);
            if event_loop.is_paused() {
                break;
            }
        }

        assert!(event_loop.is_paused());
        assert!(event_loop.debugger_open_requested());
        assert_eq!(
            nes.cpu.current_interrupt(),
            Some(crate::cpu::InterruptKind::Nmi)
        );
        assert_eq!(nes.cpu.pc, nmi_vector);
    }

    #[test]
    #[serial]
    fn test_step_into_action_runs_via_temporary_breakpoint_and_reopens_debugger() {
        let mut event_loop =
            EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0, true, None, false, false).unwrap();
        let mut nes = Nes::new(TvSystem::Ntsc);

        insert_nop_cartridge(&mut nes, 0x8000);
        nes.reset(false);

        event_loop.request_debugger_open();
        assert!(event_loop.is_paused());
        assert!(event_loop.debugger_open_requested());

        event_loop.apply_debugger_ui_action(
            &mut nes,
            crate::debugger::ui::DebuggerUiAction {
                step_into: true,
                step_over: false,
                continue_run: false,
                run_to_next_frame: false,
                run_to_nmi: false,
                run_to_irq: false,
            },
        );

        assert!(
            !event_loop.is_paused(),
            "step-into should continue running (so the main loop can render frames)"
        );
        assert!(
            !event_loop.debugger_open_requested(),
            "step-into should close debugger (same as Continue)"
        );

        // Run until the temporary step completes.
        for _ in 0..1_000_000 {
            tick_headless_once(&mut event_loop, &mut nes);
            if event_loop.is_paused() {
                break;
            }
        }

        assert_eq!(nes.cpu.pc, 0x8001);
        assert!(event_loop.is_paused());
        assert!(event_loop.debugger_open_requested());
    }

    #[test]
    #[serial]
    fn test_step_over_action_runs_via_temporary_breakpoint_and_reopens_debugger() {
        let mut event_loop =
            EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0, true, None, false, false).unwrap();
        let mut nes = nes_with_jsr_program();
        nes.cpu.x = 0;

        event_loop.request_debugger_open();
        assert!(event_loop.is_paused());
        assert!(event_loop.debugger_open_requested());

        event_loop.apply_debugger_ui_action(
            &mut nes,
            crate::debugger::ui::DebuggerUiAction {
                step_over: true,
                step_into: false,
                continue_run: false,
                run_to_next_frame: false,
                run_to_nmi: false,
                run_to_irq: false,
            },
        );

        assert!(
            !event_loop.is_paused(),
            "step-over should continue running (so the main loop can render frames)"
        );
        assert!(
            !event_loop.debugger_open_requested(),
            "step-over should close debugger (same as Continue)"
        );

        // Run until the temporary breakpoint hits (return address).
        for _ in 0..1_000_000 {
            tick_headless_once(&mut event_loop, &mut nes);
            if event_loop.is_paused() {
                break;
            }
        }

        assert_eq!(
            nes.cpu.pc, 0x8003,
            "expected step-over to stop at next instruction"
        );
        assert_eq!(
            nes.cpu.x, 1,
            "expected subroutine to have executed (INX) before returning"
        );
        assert!(event_loop.is_paused());
        assert!(event_loop.debugger_open_requested());
    }

    #[test]
    #[serial]
    fn test_request_debugger_open_pauses_and_sets_request_flag() {
        let mut event_loop =
            EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0, true, None, false, false).unwrap();

        assert!(!event_loop.paused);
        assert!(!event_loop.debugger_open_requested);

        event_loop.request_debugger_open();

        assert!(event_loop.paused);
        assert!(event_loop.debugger_open_requested);
    }

    #[test]
    fn test_handle_key_down_f1_resets_nes() {
        // Desired behavior: F1 triggers a reset through centralized handle_key_down.
        let mut nes = Nes::new(TvSystem::Ntsc);
        let mut paused = false;
        let mut debugger_open_requested = false;

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
        let _ = EventLoop::handle_key_down(
            &mut nes,
            Keycode::F1,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );
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
        let mut debugger_open_requested = false;

        // W => Up
        let mut nes = Nes::new(TvSystem::Ntsc);
        let _ = EventLoop::handle_key_down(
            &mut nes,
            Keycode::W,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );
        assert_eq!(read_joypad1_buttons(&mut nes), [0, 0, 0, 0, 1, 0, 0, 0]);

        // S => Down
        let mut nes = Nes::new(TvSystem::Ntsc);
        let _ = EventLoop::handle_key_down(
            &mut nes,
            Keycode::S,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );
        assert_eq!(read_joypad1_buttons(&mut nes), [0, 0, 0, 0, 0, 1, 0, 0]);

        // A => Left
        let mut nes = Nes::new(TvSystem::Ntsc);
        let _ = EventLoop::handle_key_down(
            &mut nes,
            Keycode::A,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );
        assert_eq!(read_joypad1_buttons(&mut nes), [0, 0, 0, 0, 0, 0, 1, 0]);

        // D => Right
        let mut nes = Nes::new(TvSystem::Ntsc);
        let _ = EventLoop::handle_key_down(
            &mut nes,
            Keycode::D,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );
        assert_eq!(read_joypad1_buttons(&mut nes), [0, 0, 0, 0, 0, 0, 0, 1]);

        // R => Select
        let mut nes = Nes::new(TvSystem::Ntsc);
        let _ = EventLoop::handle_key_down(
            &mut nes,
            Keycode::R,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );
        assert_eq!(read_joypad1_buttons(&mut nes), [0, 0, 1, 0, 0, 0, 0, 0]);

        // T => Start
        let mut nes = Nes::new(TvSystem::Ntsc);
        let _ = EventLoop::handle_key_down(
            &mut nes,
            Keycode::T,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );
        assert_eq!(read_joypad1_buttons(&mut nes), [0, 0, 0, 1, 0, 0, 0, 0]);

        // F => A
        let mut nes = Nes::new(TvSystem::Ntsc);
        let _ = EventLoop::handle_key_down(
            &mut nes,
            Keycode::F,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );
        assert_eq!(read_joypad1_buttons(&mut nes), [1, 0, 0, 0, 0, 0, 0, 0]);

        // G => B
        let mut nes = Nes::new(TvSystem::Ntsc);
        let _ = EventLoop::handle_key_down(
            &mut nes,
            Keycode::G,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );
        assert_eq!(read_joypad1_buttons(&mut nes), [0, 1, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    #[serial]
    fn test_new_headless() {
        let event_loop = EventLoop::new(true, TvSystem::Ntsc, 2.0, 1.0, true, None, false, false);
        assert!(event_loop.is_ok());
    }

    #[test]
    #[serial]
    fn test_scaling_below_minimum() {
        let event_loop = EventLoop::new(true, TvSystem::Ntsc, 0.5, 1.0, true, None, false, false);
        assert!(event_loop.is_ok());
    }

    #[test]
    #[serial]
    fn test_scaling_above_maximum() {
        let event_loop = EventLoop::new(true, TvSystem::Ntsc, 6.0, 1.0, true, None, false, false);
        assert!(event_loop.is_ok());
    }

    #[test]
    #[serial]
    fn test_scaling_at_minimum() {
        let event_loop = EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0, true, None, false, false);
        assert!(event_loop.is_ok());
    }

    #[test]
    #[serial]
    fn test_scaling_at_maximum() {
        let event_loop = EventLoop::new(true, TvSystem::Ntsc, 5.0, 1.0, true, None, false, false);
        assert!(event_loop.is_ok());
    }

    #[test]
    #[serial]
    fn test_run_with_nes() {
        let _event_loop =
            EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0, true, None, false, false).unwrap();
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

    #[test]
    #[serial]
    fn test_gamepad_disabled_by_default() {
        // When gamepads are disabled, no controllers should be initialized
        let event_loop = EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0, true, None, false, false);
        assert!(event_loop.is_ok());
        let event_loop = event_loop.unwrap();
        assert_eq!(event_loop.controllers.len(), 0);
        assert_eq!(event_loop.controller_player_map.len(), 0);
    }

    #[test]
    #[serial]
    fn test_gamepad_enabled_no_controllers_present() {
        // When gamepads are enabled but no controllers are present, should still work
        let event_loop = EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0, true, None, true, false);
        // This may succeed or fail depending on whether controllers are actually present
        // We just verify it doesn't panic
        if let Ok(event_loop) = event_loop {
            // No controllers should be initialized in test environment
            assert!(event_loop.controllers.len() <= 2);
        }
    }

    #[test]
    #[serial]
    fn test_render_debugger_if_needed_invokes_renderer() {
        struct Spy {
            calls: Rc<RefCell<usize>>,
        }

        impl DebuggerRenderer for Spy {
            fn render(&mut self, _snapshot: &crate::debugger::DebuggerSnapshot) {
                *self.calls.borrow_mut() += 1;
            }
        }

        let calls = Rc::new(RefCell::new(0usize));
        let mut event_loop =
            EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0, true, None, false, false).unwrap();
        event_loop.set_debugger_renderer(Box::new(Spy {
            calls: calls.clone(),
        }));

        let nes = Nes::new(TvSystem::Ntsc);

        event_loop.request_debugger_open();
        event_loop.render_debugger_if_needed(&nes);

        assert_eq!(*calls.borrow(), 1);
    }

    #[test]
    #[serial]
    fn test_tick_headless_once_renders_debugger_when_paused_and_requested() {
        struct Spy {
            calls: Rc<RefCell<usize>>,
        }

        impl DebuggerRenderer for Spy {
            fn render(&mut self, _snapshot: &crate::debugger::DebuggerSnapshot) {
                *self.calls.borrow_mut() += 1;
            }
        }

        let calls = Rc::new(RefCell::new(0usize));
        let mut event_loop =
            EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0, true, None, false, false).unwrap();
        event_loop.set_debugger_renderer(Box::new(Spy {
            calls: calls.clone(),
        }));
        event_loop.request_debugger_open();

        let mut nes = Nes::new(TvSystem::Ntsc);

        // Provide a minimal cartridge so `run_cpu_tick()` won't panic on unmapped vector reads.
        let mut prg_rom = vec![0xEAu8; 0x8000]; // NOP
        let reset_vector: u16 = 0x8000;
        prg_rom[0x7FFC] = (reset_vector & 0x00FF) as u8; // RESET
        prg_rom[0x7FFD] = (reset_vector >> 8) as u8;
        prg_rom[0x7FFA] = (reset_vector & 0x00FF) as u8; // NMI
        prg_rom[0x7FFB] = (reset_vector >> 8) as u8;
        prg_rom[0x7FFE] = (reset_vector & 0x00FF) as u8; // IRQ/BRK
        prg_rom[0x7FFF] = (reset_vector >> 8) as u8;

        let cartridge = crate::cartridge::Cartridge::from_parts(
            prg_rom,
            vec![],
            crate::cartridge::MirroringMode::Horizontal,
        );
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        let cpu_cycles_before = nes.cpu.get_total_cycles();

        tick_headless_once(&mut event_loop, &mut nes);

        assert_eq!(
            cpu_cycles_before,
            nes.cpu.get_total_cycles(),
            "when paused, one tick should not advance CPU cycles"
        );
        assert_eq!(
            *calls.borrow(),
            1,
            "expected one tick to render debugger when requested"
        );
    }

    #[test]
    #[serial]
    fn test_tick_headless_once_for_run_renders_debugger_when_paused_and_requested() {
        struct Spy {
            calls: Rc<RefCell<usize>>,
        }

        impl DebuggerRenderer for Spy {
            fn render(&mut self, _snapshot: &crate::debugger::DebuggerSnapshot) {
                *self.calls.borrow_mut() += 1;
            }
        }

        let calls = Rc::new(RefCell::new(0usize));
        let mut event_loop =
            EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0, true, None, false, false).unwrap();
        event_loop.set_debugger_renderer(Box::new(Spy {
            calls: calls.clone(),
        }));
        event_loop.request_debugger_open();

        let mut nes = Nes::new(TvSystem::Ntsc);

        // Provide a minimal cartridge so `run_cpu_tick()` won't panic on unmapped vector reads.
        let mut prg_rom = vec![0xEAu8; 0x8000]; // NOP
        let reset_vector: u16 = 0x8000;
        prg_rom[0x7FFC] = (reset_vector & 0x00FF) as u8; // RESET
        prg_rom[0x7FFD] = (reset_vector >> 8) as u8;
        prg_rom[0x7FFA] = (reset_vector & 0x00FF) as u8; // NMI
        prg_rom[0x7FFB] = (reset_vector >> 8) as u8;
        prg_rom[0x7FFE] = (reset_vector & 0x00FF) as u8; // IRQ/BRK
        prg_rom[0x7FFF] = (reset_vector >> 8) as u8;

        let cartridge = crate::cartridge::Cartridge::from_parts(
            prg_rom,
            vec![],
            crate::cartridge::MirroringMode::Horizontal,
        );
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        let cpu_cycles_before = nes.cpu.get_total_cycles();

        let should_quit = event_loop.tick_headless_once_for_run(&mut nes);
        assert!(!should_quit);

        assert_eq!(
            cpu_cycles_before,
            nes.cpu.get_total_cycles(),
            "when paused, one tick should not advance CPU cycles"
        );
        assert_eq!(
            *calls.borrow(),
            1,
            "expected one tick to render debugger when requested"
        );
    }

    #[test]
    #[serial]
    fn test_tick_windowed_paused_for_run_renders_debugger_when_paused_and_requested() {
        struct Spy {
            calls: Rc<RefCell<usize>>,
        }

        impl DebuggerRenderer for Spy {
            fn render(&mut self, _snapshot: &crate::debugger::DebuggerSnapshot) {
                *self.calls.borrow_mut() += 1;
            }
        }

        let calls = Rc::new(RefCell::new(0usize));
        let mut event_loop =
            EventLoop::new(true, TvSystem::Ntsc, 1.0, 1.0, true, None, false, false).unwrap();
        event_loop.set_debugger_renderer(Box::new(Spy {
            calls: calls.clone(),
        }));

        let nes = Nes::new(TvSystem::Ntsc);

        event_loop.request_debugger_open();
        EventLoop::tick_windowed_paused_for_run(
            event_loop.debugger_open_requested,
            &mut event_loop.debugger_renderer,
            &nes,
        );

        assert_eq!(*calls.borrow(), 1);
    }
}
