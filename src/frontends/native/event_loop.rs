//! Native event loop for the NES emulator.
//!
//! Uses winit's `ApplicationHandler` to drive the emulation loop
//! with rendering via `NativeGlWrapper` and audio via `NativeAudio`.

use crate::frontends::native::app_state::NativeAppState;
use crate::frontends::native::audio::NativeAudio;
use crate::frontends::native::gamepad::{GamepadChange, GamepadManager};
use crate::frontends::native::gl_wrapper::NativeGlWrapper;
use crate::frontends::native::keyboard::{self, KeyOutcome};
use crate::frontends::native::mouse;
use crate::frontends::native::sleep_inhibitor::SleepInhibitor;
use crate::gb::debugging::control::GbDebuggerController;
use crate::nes::debugging::control::DebuggerController;
use crate::platform::app_context::SharedAppContext;
use crate::platform::audio::{EmulatorAudio, normalize_nes_sample};
use crate::platform::autorun::AutorunMode;
use crate::platform::autorun::state::AutorunState;
use crate::platform::debugging::Tracing;
use crate::platform::emulator::{Console, Emulator, SystemType};
use crate::platform::frontend_toasts::{
    gamepad_connected_toast_message, gamepad_disconnected_toast_message, gamepad_init_toast_message,
};

use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::WindowId;

use std::time::{Duration, Instant};

/// Native event loop that runs the NES emulator using winit + glutin.
pub struct NativeEventLoop {
    app_context: SharedAppContext,
    console: Console,
    audio: Option<NativeAudio>,
    tracing: Tracing,
    state: NativeAppState,
    debugger_controller: DebuggerController,
    gb_debugger_controller: GbDebuggerController,
    /// Whether the user had manually paused before the debugger opened,
    /// so we can restore pause state when the debugger closes.
    paused_before_debugger: bool,
    /// Whether the user had manually paused before the cart-switch dialog opened.
    paused_before_cart_switch: bool,
    gamepad: Option<GamepadManager>,
    gamepads_enabled: bool,
    gamepad_init_toast_shown: bool,
    gamepad_init_failed: bool,
    sleep_inhibitor: Option<SleepInhibitor>,

    // Initialized on resume (when the window is ready)
    gl_wrapper: Option<NativeGlWrapper>,
    last_audio_stats_print: Instant,
    initialized: bool,

    autorun_state: Option<AutorunState>,
    /// Set when autorun playback completes; the exit string is propagated on next redraw.
    autorun_exit: Option<String>,
    /// Whether to run without a window (headless autorun mode).
    headless: bool,
    /// Whether VSync is enabled (glutin swap interval handles timing).
    vsync_enabled: bool,
    /// Deadline for the next frame, used for manual frame limiting when VSync is off.
    next_frame_deadline: Instant,
    /// Time of the last executed emulation frame, used to guard against
    /// OS-triggered redraws arriving faster than our WaitUntil deadline
    /// (e.g. macOS ProMotion delivering RedrawRequested at 120 Hz).
    last_frame_rendered: Instant,
    /// Ring buffer of recent frame timestamps for FPS calculation.
    frame_timestamps: std::collections::VecDeque<Instant>,
}

impl NativeEventLoop {
    pub fn new(
        app_context: SharedAppContext,
        console: Console,
        audio: Option<NativeAudio>,
        tracing: Tracing,
        headless: bool,
    ) -> Self {
        let (
            gamepads_enabled,
            four_score,
            fullscreen,
            vsync_enabled,
            debugger_controller,
            gb_debugger_controller,
        ) = {
            let config = app_context.borrow().config().clone();
            let dc = DebuggerController::new(
                &config.frontend.breakpoints,
                config.frontend.debugger_enabled,
            );
            let gb_dc = GbDebuggerController::new(
                &config.frontend.breakpoints,
                config.frontend.debugger_enabled,
            );
            (
                config.frontend.gamepads_enabled,
                config.nes.four_score_enabled,
                config.frontend.fullscreen,
                config.frontend.vsync_enabled,
                dc,
                gb_dc,
            )
        };

        let (gamepad, gamepad_init_failed) = if gamepads_enabled {
            match GamepadManager::new(four_score) {
                Ok(gp) => (Some(gp), false),
                Err(e) => {
                    crate::platform::debugging::log_info(format!("Gamepad init failed: {e}"));
                    (None, true)
                }
            }
        } else {
            (None, false)
        };

        let sleep_inhibitor = match SleepInhibitor::new() {
            Ok(si) => Some(si),
            Err(e) => {
                crate::platform::debugging::log_info(format!("Sleep inhibitor init failed: {e}"));
                None
            }
        };

        Self {
            app_context,
            console,
            audio,
            tracing,
            state: NativeAppState {
                fullscreen,
                four_score_enabled: four_score,
                window_focused: true,
                ..NativeAppState::default()
            },
            debugger_controller,
            gb_debugger_controller,
            paused_before_debugger: false,
            paused_before_cart_switch: false,
            gamepad,
            gamepads_enabled,
            gamepad_init_toast_shown: false,
            gamepad_init_failed,
            sleep_inhibitor,
            gl_wrapper: None,
            last_audio_stats_print: Instant::now(),
            initialized: false,
            autorun_state: None,
            autorun_exit: None,
            headless,
            vsync_enabled,
            next_frame_deadline: Instant::now(),
            last_frame_rendered: Instant::now(),
            frame_timestamps: std::collections::VecDeque::new(),
        }
    }

    pub fn run(self) -> Result<(), String> {
        if self.headless {
            return self.run_headless();
        }
        let mut event_loop =
            EventLoop::new().map_err(|e| format!("Failed to create event loop: {e}"))?;
        self.run_with_event_loop(&mut event_loop)
    }

    /// Run with an externally-created event loop (used when transitioning from ROM browser).
    pub fn run_with_event_loop(mut self, event_loop: &mut EventLoop<()>) -> Result<(), String> {
        use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
        event_loop
            .run_app_on_demand(&mut self)
            .map_err(|e| format!("Event loop error: {e}"))?;
        // Propagate deferred autorun exit if set during the event loop.
        if let Some(exit_str) = self.autorun_exit.take() {
            return Err(exit_str);
        }
        Ok(())
    }

    fn initialize_audio(&mut self) {
        if let Some(ref mut audio) = self.audio {
            let startup_prime_samples = audio.startup_prime_samples();
            audio.prime_startup(startup_prime_samples);
            audio.resume();
        }
    }

    fn debugger_paused(&self) -> bool {
        match self.console.system_type() {
            SystemType::Nes => self.debugger_controller.is_paused(),
            SystemType::GameBoy => self.gb_debugger_controller.is_paused(),
            SystemType::Gba | SystemType::Snes => false,
        }
    }

    fn debugger_open(&self) -> bool {
        match self.console.system_type() {
            SystemType::Nes => self.debugger_controller.is_debugger_open(),
            SystemType::GameBoy => self.gb_debugger_controller.is_debugger_open(),
            SystemType::Gba | SystemType::Snes => false,
        }
    }

    fn run_frame(&mut self) {
        let debugger_paused = self.debugger_paused();
        if self.state.paused && !debugger_paused {
            // Manually paused (not debugger) — skip frame
            return;
        }

        // Apply button states from autorun before emulation
        match self.handle_autorun_before_frame() {
            Ok(true) => {}
            Ok(false) => {
                let _ = self.finish_recording();
                return;
            }
            Err(exit_str) => {
                self.autorun_exit = Some(exit_str);
                return;
            }
        }

        // Record button states after input processing
        let autorun_checkpoint_due = self.handle_autorun_after_input();

        let audio = self.audio.take();
        let audio_cell = std::cell::RefCell::new(audio);
        if let Some(nes) = self.console.as_nes_mut() {
            self.debugger_controller
                .run_frame(nes, &self.tracing, &mut |nes| {
                    if let Some(ref mut audio) = *audio_cell.borrow_mut() {
                        while nes.sample_ready() {
                            if let Some(sample) = nes.get_sample() {
                                audio.queue_sample(normalize_nes_sample(sample));
                            }
                        }
                    }
                });
        } else if let Some(gb) = self.console.as_gameboy_mut() {
            gb.run_frame_with_debugger(&mut self.gb_debugger_controller, &audio_cell);
        } else if let Some(gba) = self.console.as_gba_mut() {
            while !gba.is_ready_to_render() {
                let _ = gba.run_tick();
                if let Some(ref mut audio) = *audio_cell.borrow_mut() {
                    while gba.sample_ready() {
                        if let Some((left, right)) = gba.get_stereo_sample() {
                            audio.queue_stereo_sample(left, right);
                        }
                    }
                }
            }
        } else if let Some(snes) = self.console.as_snes_mut() {
            while !snes.is_ready_to_render() {
                let _ = snes.run_tick();
                if let Some(ref mut audio) = *audio_cell.borrow_mut() {
                    while snes.sample_ready() {
                        if let Some(sample) = snes.get_sample() {
                            audio.queue_sample(sample);
                        }
                    }
                }
            }
        }
        self.audio = audio_cell.into_inner();
        self.sync_from_controller();
        self.console.clear_ready_to_render();

        self.handle_autorun_after_frame(autorun_checkpoint_due);

        // Log audio stats every second
        if let Some(ref audio) = self.audio
            && self.last_audio_stats_print.elapsed() >= Duration::from_secs(1)
        {
            let (received, dropped, underrun) = audio.take_and_reset_stats();
            if dropped != 0 || underrun != 0 {
                crate::platform::debugging::log_info(format!(
                    "Audio stats (last ~1s): received={received}, dropped={dropped}, underrun={underrun}"
                ));
            }
            self.last_audio_stats_print = Instant::now();
        }
    }

    /// Syncs the audio device's paused/playing state to match the current
    /// window focus and debugger state.  Should be called whenever either
    /// changes to avoid underruns while paused.
    fn sync_audio_state(&self) {
        if let Some(ref audio) = self.audio {
            if audio_should_be_paused(self.state.window_focused, self.debugger_paused()) {
                audio.pause();
            } else {
                audio.resume();
            }
        }
    }

    /// Sync frontend state from the debugger controller.
    fn sync_from_controller(&mut self) {
        let debugger_open = self.debugger_open();

        if debugger_open {
            if !self.state.debugger_open {
                // Debugger just opened — remember manual pause state.
                self.paused_before_debugger = self.state.paused;
            }
            self.state.paused = true;
            self.state.debugger_open = true;
        } else if self.state.debugger_open {
            // Debugger just closed — restore manual pause state.
            self.state.paused = self.paused_before_debugger;
            self.state.debugger_open = false;
        }
        self.sync_audio_state();
    }

    /// Synchronizes the actual mouse grab state with the desired state.
    ///
    /// Called once per frame to ensure grab/visibility stay in sync after
    /// cartridge switches, focus changes, or controller hot-swaps.
    fn sync_mouse_grab_state(&mut self) {
        let has_mouse = mouse::has_any_mouse_controller(&self.console);
        let should_grab = crate::nes::input::mouse_mapping::should_grab_mouse_input(
            has_mouse,
            self.state.window_focused,
            self.state.mouse_released_by_escape,
        );

        if self.state.mouse_grabbed != should_grab {
            if let Some(ref mut gl) = self.gl_wrapper {
                if should_grab {
                    // Use Locked for ALL controllers — on macOS, Confined is
                    // not supported, so the cursor would escape the window.
                    // Locked keeps the cursor pinned and we track position
                    // ourselves via DeviceEvent::MouseMotion deltas.
                    let _ = gl.set_mouse_grab_locked();
                    gl.window().set_cursor_visible(false);
                    // Centre virtual cursor and immediately sync NES coords so
                    // there is no stale position before the first delta arrives.
                    let (w, h) = gl.window_size();
                    let cx = w as f32 / 2.0;
                    let cy = h as f32 / 2.0;
                    self.state.virtual_cursor = (cx, cy);
                    self.state.last_zapper_position =
                        mouse::update_mouse_motion(&mut self.console, cx as i32, cy as i32, w, h);
                } else {
                    let _ = gl.set_mouse_grab(false);
                    gl.window().set_cursor_visible(true);
                }
            }
            self.state.mouse_grabbed = should_grab;
        }
    }

    // ── Cartridge switching ──────────────────────────────────────────────────

    /// Opens the cartridge-switch dialog, loading the catalog CSV first.
    fn open_cartridge_switch_dialog(&mut self) {
        self.paused_before_cart_switch = self.state.paused;
        self.state.cart_switch.open = true;
        self.state.cart_switch.filter.clear();
        self.state.cart_switch.selection = 0;

        if self.state.cart_switch.entries.is_empty() {
            self.load_catalog_entries();
        }

        self.state.paused = true;
    }

    /// Restores the pause state that was active before the dialog opened.
    fn restore_pause_after_cart_switch(&mut self) {
        self.state.paused = self.state.debugger_open || self.paused_before_cart_switch;
        self.paused_before_cart_switch = false;
    }

    /// Loads cartridge catalog entries from the default CSV path.
    fn load_catalog_entries(&mut self) {
        if let Some(home) = std::env::var_os("HOME") {
            let catalog_path = crate::nes::console::default_catalog_csv_path(
                std::path::PathBuf::from(home).as_path(),
            );
            if let Ok(content) = std::fs::read_to_string(&catalog_path) {
                self.state.cart_switch.entries = content
                    .lines()
                    .skip(1)
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(ToString::to_string)
                    .collect();
            }
        }
    }

    /// Loads a new cartridge from the given ROM path and resets the emulator.
    fn switch_to_cartridge(&mut self, rom_path: &str) {
        let Some(_) = self.console.as_nes() else {
            return;
        };
        let rom_bytes = match std::fs::read(rom_path) {
            Ok(bytes) => bytes,
            Err(err) => {
                crate::platform::debugging::log_info(format!("Failed to read ROM: {err}"));
                return;
            }
        };

        let cartridge = {
            let Some(nes) = self.console.as_nes() else {
                unreachable!("console type checked above");
            };
            match crate::nes::cartridge::Cartridge::load_from_file(
                &rom_bytes,
                rom_path,
                Some(nes.rom_db()),
            ) {
                Ok(c) => c,
                Err(err) => {
                    crate::platform::debugging::log_info(format!(
                        "Failed to load ROM cartridge: {err}"
                    ));
                    return;
                }
            }
        };

        let applied = {
            let rom_timing = cartridge.rom_timing_mode();
            let app_context = self.console.app_context().clone();
            app_context
                .borrow_mut()
                .config_mut()
                .apply_rom_timing_mode(rom_timing)
        };

        if let Some(nes) = self.console.as_nes_mut() {
            nes.insert_cartridge(cartridge);
        }
        crate::nes::console::log_hardware_selection(self.console.app_context(), applied);
        self.console.reset(false);
    }

    // ── Autorun ──────────────────────────────────────────────────────────────

    /// Initialize autorun recording or playback.
    #[allow(clippy::too_many_arguments)]
    pub fn init_autorun(
        &mut self,
        mode: AutorunMode,
        rom_path: &str,
        overwrite: bool,
        extend: bool,
        from_checkpoint: Option<i64>,
        format: crate::platform::autorun::AutorunFormat,
    ) -> Result<(), String> {
        if mode != AutorunMode::None {
            let (state, pending) =
                AutorunState::new(mode, rom_path, overwrite, extend, from_checkpoint, format)?;
            if let Some(restore) = pending {
                let save_state =
                    crate::nes::console::SaveState::from_bytes(&restore.state_bytes)
                        .map_err(|e| format!("Failed to deserialize checkpoint state: {e}"))?;
                if let Some(nes) = self.console.as_nes_mut() {
                    nes.load_state(&save_state)
                        .map_err(|e| format!("Failed to restore checkpoint state: {e}"))?;
                }
            }
            self.autorun_state = Some(state);
        }
        Ok(())
    }

    /// Handle autorun logic before emulating a frame.
    ///
    /// In playback or extend mode, applies button states from the recording.
    /// Returns `Ok(true)` if emulation should continue, or `Err(exit_string)`
    /// when playback completes.
    fn handle_autorun_before_frame(&mut self) -> Result<bool, String> {
        let Some(ref mut autorun_state) = self.autorun_state else {
            return Ok(true);
        };

        autorun_state.begin_frame();

        if autorun_state.mode() == AutorunMode::Playback || autorun_state.is_extending_playback() {
            if let Some(frame) = autorun_state.next_playback_frame() {
                self.console.set_joypad_button_states(1, frame.player1);
                self.console.set_joypad_button_states(2, frame.player2);
                return Ok(true);
            }

            if autorun_state.mode() == AutorunMode::Playback {
                return self.finish_playback();
            }
        }

        Ok(true)
    }

    /// Handle autorun logic after processing input but before rendering.
    ///
    /// In record mode, captures current button states. Returns `true` if a
    /// checkpoint should be captured after the frame is fully rendered.
    fn handle_autorun_after_input(&mut self) -> bool {
        let Some(ref mut autorun_state) = self.autorun_state else {
            return false;
        };

        if autorun_state.mode() == AutorunMode::Record
            && !autorun_state.is_extending_playback()
            && !autorun_state.current_frame_was_prerecorded()
        {
            let player1 = self.console.get_joypad_button_states(1);
            let player2 = self.console.get_joypad_button_states(2);
            return autorun_state.record_frame(player1, player2);
        }
        false
    }

    /// Handle autorun actions after a frame has been fully rendered.
    ///
    /// Captures checkpoints in record mode and verifies CRCs in playback mode.
    fn handle_autorun_after_frame(&mut self, checkpoint_due: bool) {
        if checkpoint_due {
            let crc = self.console.screen_crc32();
            let state_bytes = self.console.save_state_bytes().unwrap_or_default();
            if let Some(ref mut autorun_state) = self.autorun_state {
                autorun_state.record_checkpoint(crc, state_bytes);
            }
        }

        if let Some(ref mut autorun_state) = self.autorun_state
            && (autorun_state.mode() == AutorunMode::Playback
                || autorun_state.is_extending_playback())
        {
            let crc = self.console.screen_crc32();
            if let Some(matched) = autorun_state.check_playback_checkpoint(crc) {
                let current_frame = autorun_state.current_frame_index();
                let total_frames = autorun_state.total_frames();
                let current_checkpoint = autorun_state.total_checkpoints_verified();
                let total_checkpoints = autorun_state.total_checkpoints();
                if matched {
                    crate::platform::debugging::log_info(format!(
                        "Autorun checkpoint CRC match (0x{crc:08X}) at frame {current_frame}/{total_frames}, checkpoint {current_checkpoint}/{total_checkpoints}",
                    ));
                } else {
                    crate::platform::debugging::log_info(format!(
                        "Autorun checkpoint CRC MISMATCH at frame {current_frame}/{total_frames}, checkpoint {current_checkpoint}/{total_checkpoints}: got 0x{crc:08X}",
                    ));
                }
            }
        }
    }

    /// Finish playback by reporting CRC verification results.
    fn finish_playback(&mut self) -> Result<bool, String> {
        let Some(ref autorun_state) = self.autorun_state else {
            return Ok(true);
        };

        let mismatches = autorun_state.crc_mismatches();
        let verified = autorun_state.total_checkpoints_verified();
        let crc = self.console.screen_crc32();

        if mismatches == 0 {
            crate::platform::debugging::log_info(format!(
                "Autorun playback successful: {verified} checkpoints verified, final CRC 0x{crc:08X}",
            ));
            Err("AUTORUN_EXIT:0".to_string())
        } else {
            crate::platform::debugging::log_info(format!(
                "Autorun playback failed: {mismatches}/{verified} CRC mismatches",
            ));
            Err("AUTORUN_EXIT:1".to_string())
        }
    }

    /// Finish recording by saving the autorun file with a final checkpoint.
    fn finish_recording(&mut self) -> Result<(), String> {
        let Some(ref mut autorun_state) = self.autorun_state else {
            return Ok(());
        };

        if autorun_state.mode() != AutorunMode::Record {
            return Ok(());
        }

        let crc = self.console.screen_crc32();
        let state_bytes = self.console.save_state_bytes().unwrap_or_default();

        autorun_state.save_with_final_checkpoint(crc, state_bytes)?;
        crate::platform::debugging::log_info(format!(
            "Autorun recording saved: {} frames, final CRC 0x{crc:08X}",
            autorun_state.total_frames(),
        ));

        Ok(())
    }

    /// Run in headless mode (no window, no GL, no audio). Returns when playback
    /// finishes or recording is interrupted.
    pub fn run_headless(mut self) -> Result<(), String> {
        loop {
            match self.handle_autorun_before_frame() {
                Ok(true) => {}
                Ok(false) => {
                    self.finish_recording()?;
                    return Ok(());
                }
                Err(exit_str) => return Err(exit_str),
            }

            let checkpoint_due = self.handle_autorun_after_input();

            if let Some(nes) = self.console.as_nes_mut() {
                crate::nes::autorun::headless_playback::run_one_frame(nes);
            }

            self.handle_autorun_after_frame(checkpoint_due);
        }
    }
}

impl ApplicationHandler for NativeEventLoop {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gl_wrapper.is_some() {
            return;
        }

        match NativeGlWrapper::new(
            event_loop,
            self.app_context.clone(),
            self.console.system_type(),
            self.console.allowed_shaders(),
        ) {
            Ok(gl) => {
                self.gl_wrapper = Some(gl);
                if !self.initialized {
                    self.initialize_audio();
                    self.sync_audio_state();
                    if let Some(nes) = self.console.as_nes() {
                        let watches = self.debugger_controller.load_debug_state_from_file(nes);
                        if let Some(ref mut gl) = self.gl_wrapper {
                            gl.set_watch_addresses(watches);
                        }
                    }
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
            WindowEvent::CloseRequested => {
                if let Err(e) = self.finish_recording() {
                    eprintln!("Failed to finish recording on window close: {e}");
                }
                if let Some(nes) = self.console.as_nes() {
                    let watches = self
                        .gl_wrapper
                        .as_ref()
                        .map(|gl| gl.watch_addresses())
                        .unwrap_or_default();
                    self.debugger_controller
                        .save_debug_state_to_file(nes, &watches);
                }
                if let Err(e) = self.console.save_ram() {
                    eprintln!("Failed to save battery-backed RAM on exit: {e}");
                }
                event_loop.exit();
            }

            // Resize the glutin surface whenever the physical window size changes.
            // This covers both manual window resizes and fullscreen transitions.
            // Without this, the GL surface stays at the original windowed size and
            // the rendered image is clipped instead of filling the display.
            WindowEvent::Resized(physical_size) => {
                if let Some(ref mut gl) = self.gl_wrapper {
                    gl.notify_resize(physical_size.width, physical_size.height);
                }
            }

            WindowEvent::Focused(focused) => {
                self.state.window_focused = focused;
                self.sync_audio_state();
                if !focused {
                    // Release grab on focus loss, but do NOT set
                    // mouse_released_by_escape — that flag is only for
                    // explicit Escape key presses. Keeping it clear means
                    // auto-grab resumes when focus returns, which is the
                    // right behaviour. (On macOS, Focused(false) also fires
                    // briefly during window initialisation, so setting the
                    // flag here would permanently block auto-grab.)
                    self.state.mouse_grabbed = false;
                    if let Some(ref mut gl) = self.gl_wrapper {
                        let _ = gl.set_mouse_grab(false);
                        gl.window().set_cursor_visible(true);
                    }
                }
            }

            WindowEvent::ModifiersChanged(mods) => {
                let state = mods.state();
                self.state.modifiers = state;
                if let Some(ref mut gl) = self.gl_wrapper {
                    gl.handle_modifiers_changed(state);
                }
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if event.repeat {
                    return;
                }

                if let Some(ref mut gl) = self.gl_wrapper {
                    gl.handle_key_event(&event);
                    // Forward character text to UI debugger input fields
                    // (breakpoint address, memory-watch).  Without this, typing
                    // in debugger text fields has no effect (#1859).
                    if self.state.keyboard_captured_by_ui()
                        && let Some(ref text) = event.text
                    {
                        gl.handle_text_input(text.to_string());
                    }
                }

                let PhysicalKey::Code(key_code) = event.physical_key else {
                    return;
                };

                let mouse_grabbed_before = self.state.mouse_grabbed;

                if event.state == ElementState::Pressed {
                    let fullscreen_before = self.state.fullscreen;
                    let audio_ref: Option<&dyn EmulatorAudio> =
                        self.audio.as_ref().map(|a| a as &dyn EmulatorAudio);
                    let outcome = keyboard::handle_key_pressed(
                        &mut self.console,
                        key_code,
                        &mut self.state,
                        audio_ref,
                    );
                    match outcome {
                        KeyOutcome::Quit => {
                            if let Err(e) = self.finish_recording() {
                                eprintln!("Failed to finish recording on quit: {e}");
                            }
                            if let Some(nes) = self.console.as_nes() {
                                let watches = self
                                    .gl_wrapper
                                    .as_ref()
                                    .map(|gl| gl.watch_addresses())
                                    .unwrap_or_default();
                                self.debugger_controller
                                    .save_debug_state_to_file(nes, &watches);
                            }
                            if let Err(e) = self.console.save_ram() {
                                eprintln!("Failed to save battery-backed RAM on quit: {e}");
                            }
                            event_loop.exit();
                        }
                        KeyOutcome::CycleShader => {
                            if let Some(ref mut gl) = self.gl_wrapper {
                                let preset_name = gl.cycle_shader();
                                let toast =
                                    crate::frontends::native::gl_backend::shader_toast_message(
                                        preset_name.as_deref(),
                                    );
                                self.console.app_context().borrow_mut().add_toast(toast);
                            }
                        }
                        KeyOutcome::ToggleDebugger => {
                            if let Some(nes) = self.console.as_nes_mut() {
                                self.debugger_controller.toggle_debugger(nes);
                            } else if let Some(gb) = self.console.as_gameboy_mut() {
                                gb.toggle_debugger_with_controller(
                                    &mut self.gb_debugger_controller,
                                );
                            }
                            self.sync_from_controller();
                        }
                        KeyOutcome::StepOver => {
                            if let Some(nes) = self.console.as_nes_mut() {
                                self.debugger_controller.step_over(nes);
                            } else if let Some(gb) = self.console.as_gameboy_mut() {
                                gb.step_over_with_controller(&mut self.gb_debugger_controller);
                            }
                            self.sync_from_controller();
                        }
                        KeyOutcome::StepInto => {
                            if let Some(nes) = self.console.as_nes_mut() {
                                self.debugger_controller.step_into(nes);
                            } else if let Some(gb) = self.console.as_gameboy_mut() {
                                gb.step_into_with_controller(&mut self.gb_debugger_controller);
                            }
                            self.sync_from_controller();
                        }
                        KeyOutcome::SwitchCartridge(path) => {
                            self.switch_to_cartridge(&path);
                            self.restore_pause_after_cart_switch();
                        }
                        KeyOutcome::OpenCartridgeSwitch => {
                            self.open_cartridge_switch_dialog();
                        }
                        KeyOutcome::CloseCartridgeSwitch => {
                            self.restore_pause_after_cart_switch();
                        }
                        KeyOutcome::Continue => {
                            if self.state.fullscreen != fullscreen_before
                                && let Some(ref mut gl) = self.gl_wrapper
                            {
                                let _ = gl.set_fullscreen(self.state.fullscreen);
                            }
                        }
                        KeyOutcome::ToggleFps => {
                            self.state.show_fps = !self.state.show_fps;
                        }
                        KeyOutcome::CyclePalette => {
                            if let Some(nes) = self.console.as_nes_mut() {
                                let palette = nes.cycle_palette();
                                let toast =
                                    crate::nes::frontend_toasts::palette_toast_message(palette);
                                self.console.app_context().borrow_mut().add_toast(&toast);
                            }
                        }
                    }
                } else {
                    keyboard::handle_key_released(
                        &mut self.console,
                        key_code,
                        self.state.gamepad_count,
                        self.state.four_score_enabled,
                    );
                }

                // If keyboard handler released the mouse grab (Escape), apply it.
                if mouse_grabbed_before
                    && !self.state.mouse_grabbed
                    && let Some(ref mut gl) = self.gl_wrapper
                {
                    let _ = gl.set_mouse_grab(false);
                    gl.window().set_cursor_visible(true);
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                // Forward to the UI layer always.
                if let Some(ref mut gl) = self.gl_wrapper {
                    gl.handle_cursor_moved(position);
                }
                // When grabbed, all position input comes via DeviceEvent::MouseMotion
                // (accumulated into virtual_cursor). CursorMoved is unreliable in
                // Locked grab mode — the reported position is always the lock point.
            }

            WindowEvent::MouseInput { button, state, .. } => {
                // Forward to the UI layer.
                if let Some(ref mut gl) = self.gl_wrapper {
                    gl.handle_mouse_button(button, state);
                }

                let has_mouse = mouse::has_any_mouse_controller(&self.console);

                // Left-click grabs immediately so the same click is also forwarded
                // as a button press (unlike deferring to the next frame, which
                // would swallow Zapper shots and Arkanoid trigger presses).
                // Exception: if the mouse was released by Escape, the click only
                // re-grabs and must NOT be forwarded to the NES controller.
                let mut should_discard_grab_click = false;
                if has_mouse
                    && !self.state.mouse_grabbed
                    && state == ElementState::Pressed
                    && button == winit::event::MouseButton::Left
                {
                    let was_released_by_escape = self.state.mouse_released_by_escape;
                    self.state.mouse_released_by_escape = false;
                    let should_grab = crate::nes::input::mouse_mapping::should_grab_mouse_input(
                        true,
                        self.state.window_focused,
                        false,
                    );
                    if should_grab {
                        if let Some(ref mut gl) = self.gl_wrapper {
                            let _ = gl.set_mouse_grab_locked();
                            gl.window().set_cursor_visible(false);
                            // Centre virtual cursor and immediately sync NES coords.
                            let (w, h) = gl.window_size();
                            let cx = w as f32 / 2.0;
                            let cy = h as f32 / 2.0;
                            self.state.virtual_cursor = (cx, cy);
                            self.state.last_zapper_position = mouse::update_mouse_motion(
                                &mut self.console,
                                cx as i32,
                                cy as i32,
                                w,
                                h,
                            );
                        }
                        self.state.mouse_grabbed = true;
                        if !mouse::should_forward_grab_click(was_released_by_escape) {
                            should_discard_grab_click = true;
                        }
                    }
                }

                // Route button to NES controller if grabbed (but not for the
                // re-grab click itself, which is silently discarded).
                if has_mouse && self.state.mouse_grabbed && !should_discard_grab_click {
                    let btn = match button {
                        winit::event::MouseButton::Left => Some(mouse::MouseButton::Left),
                        winit::event::MouseButton::Right => Some(mouse::MouseButton::Right),
                        _ => None,
                    };
                    if let Some(btn) = btn {
                        mouse::update_mouse_button(
                            &mut self.console,
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
                // If autorun signalled exit during the last frame, exit now.
                if self.autorun_exit.is_some() {
                    event_loop.exit();
                    return;
                }

                // When using manual frame throttle (vsync off or window unfocused),
                // the OS (e.g. macOS ProMotion at 120 Hz) may deliver RedrawRequested
                // more frequently than our WaitUntil deadline. Guard against double-
                // stepping the emulator by skipping emulation if not enough time has
                // elapsed since the last frame.
                let using_manual_throttle =
                    should_use_manual_frame_throttle(self.vsync_enabled, self.state.window_focused);
                // The Game Boy has no audio output, so there is no ring-buffer
                // back-pressure to limit the frame rate in vsync+focused mode.
                // Likewise, when the NES runs with audio disabled (--no-audio),
                // there is no ring-buffer back-pressure either.
                // Apply the same elapsed-time guard in both cases.
                let is_game_boy = self.console.system_type() == SystemType::GameBoy;
                let audio_disabled = self.audio.is_none();
                let frame_guard =
                    needs_frame_guard(using_manual_throttle, is_game_boy, audio_disabled);
                let skip_emulation = if frame_guard {
                    let target = self.console.target_frame_duration();
                    // Allow a small tolerance (half a frame) to avoid skipping
                    // frames when the deadline fires slightly early.
                    self.last_frame_rendered.elapsed() < target / 2
                } else {
                    false
                };

                if !skip_emulation {
                    // Run one frame of emulation
                    self.run_frame();
                    self.last_frame_rendered = Instant::now();
                    // Record timestamp for FPS counter (only real emulation frames).
                    self.frame_timestamps.push_back(self.last_frame_rendered);
                    let window = std::time::Duration::from_secs(1);
                    while self.frame_timestamps.front().is_some_and(|t| {
                        self.last_frame_rendered.saturating_duration_since(*t) > window
                    }) {
                        self.frame_timestamps.pop_front();
                    }
                }

                // If autorun signalled exit during this frame, exit now.
                if self.autorun_exit.is_some() {
                    event_loop.exit();
                    return;
                }

                // Sync mouse grab state each frame.
                self.sync_mouse_grab_state();

                // Render and apply debugger UI actions
                let action = if let Some(ref mut gl) = self.gl_wrapper {
                    if self.console.as_nes().is_some() {
                        gl.update_breakpoints(self.debugger_controller.breakpoints());
                    } else if self.console.as_gameboy().is_some() {
                        gl.update_gb_breakpoints(self.gb_debugger_controller.breakpoints());
                    }
                    let crosshair =
                        mouse::zapper_crosshair(&self.console, self.state.last_zapper_position);
                    let overlay = self
                        .state
                        .overlay_text(&self.console, self.autorun_state.as_ref());

                    let fps = self.frame_timestamps.len();

                    gl.render(
                        &self.console,
                        self.state.debugger_open,
                        overlay.as_deref(),
                        false,
                        crosshair,
                        if self.state.show_fps { Some(fps) } else { None },
                    )
                } else {
                    Default::default()
                };
                if let Some(nes) = self.console.as_nes_mut() {
                    self.debugger_controller.apply_ui_action(nes, action);
                } else if let Some(gb) = self.console.as_gameboy_mut()
                    && let Some(ref mut gl) = self.gl_wrapper
                {
                    let gb_action = gl.take_gb_debugger_action();
                    gb.apply_ui_action_with_controller(&mut self.gb_debugger_controller, gb_action);
                }
                self.sync_from_controller();
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
        // DeviceEvent::MouseMotion delivers raw deltas regardless of whether
        // the cursor is inside the window — this is the winit equivalent of
        // SDL2's SDL_CaptureMouse(true) + SDL_SetRelativeMouseMode(false).
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            if !self.state.mouse_grabbed {
                return;
            }

            let (w, h) = self
                .gl_wrapper
                .as_ref()
                .map(|gl| gl.window_size())
                .unwrap_or((320, 240));

            if mouse::has_snes_mouse(&self.console) && !mouse::has_zapper(&self.console) {
                // SNES Mouse: pass raw deltas directly.
                // Zapper takes precedence — if a Zapper is also connected,
                // fall through to the virtual-cursor path (matching SDL logic).
                mouse::apply_snes_mouse_relative_motion(
                    &mut self.console,
                    dx as i32,
                    dy as i32,
                    w,
                    h,
                );
            } else {
                // Zapper / Arkanoid: accumulate deltas into a virtual cursor
                // position clamped to the window, then map to NES coordinates.
                // This replicates SDL2's behaviour where absolute x,y was still
                // available because SDL2 synthesised it from deltas internally.
                let (new_vx, new_vy) = mouse::accumulate_virtual_cursor(
                    self.state.virtual_cursor,
                    dx as f32,
                    dy as f32,
                    w,
                    h,
                );
                self.state.virtual_cursor = (new_vx, new_vy);

                self.state.last_zapper_position = mouse::update_mouse_motion(
                    &mut self.console,
                    new_vx as i32,
                    new_vy as i32,
                    w,
                    h,
                );
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Poll gamepad events before requesting redraw.
        if let Some(ref mut gp) = self.gamepad {
            let changes = gp.process_events(&mut self.console);
            self.state.gamepad_count = gp.connected_count();

            if self.gamepad_init_toast_shown {
                // Show hot-plug toasts. The init toast guard avoids
                // double-toasting the startup connections on macOS where
                // IOKit enumerates asynchronously.
                for change in changes {
                    let toast = match change {
                        GamepadChange::Connected(p) => gamepad_connected_toast_message(p),
                        GamepadChange::Disconnected(p) => gamepad_disconnected_toast_message(p),
                    };
                    self.app_context.borrow_mut().add_toast(&toast);
                }
            } else {
                // Show the one-shot init toast on the first process_events() call.
                self.gamepad_init_toast_shown = true;
                let toast = if self.gamepad_init_failed {
                    "Gamepad init failed: using keyboard controls".to_string()
                } else {
                    gamepad_init_toast_message(self.gamepads_enabled, gp.connected_count())
                };
                self.app_context.borrow_mut().add_toast(&toast);
            }
        }

        if let Some(ref gl) = self.gl_wrapper {
            if self.state.paused {
                // Throttle to ~20fps while paused to avoid spinning the CPU/GPU.
                event_loop.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + Duration::from_millis(50),
                ));
                // Allow display to sleep when paused.
                if let Some(ref mut si) = self.sleep_inhibitor {
                    si.deactivate();
                }
            } else if should_use_manual_frame_throttle(
                self.vsync_enabled,
                self.state.window_focused,
            ) || self.console.as_gameboy().is_some()
            {
                // Manual frame limiting: advance deadline by target interval.
                // The GameBoy branch is included here because it has no audio
                // back-pressure to naturally throttle the frame rate; without
                // an explicit WaitUntil the event loop would Poll at the
                // display's full refresh rate (e.g. 120 Hz on ProMotion Macs).
                let target = self.console.target_frame_duration();
                self.next_frame_deadline += target;
                // Clamp to at least now to avoid spinning on past deadlines.
                let now = Instant::now();
                if self.next_frame_deadline < now {
                    self.next_frame_deadline = now;
                }
                event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame_deadline));
                // Prevent display from sleeping while emulating.
                if let Some(ref mut si) = self.sleep_inhibitor {
                    si.activate();
                }
            } else {
                event_loop.set_control_flow(ControlFlow::Poll);
                // Prevent display from sleeping while emulating.
                if let Some(ref mut si) = self.sleep_inhibitor {
                    si.activate();
                }
            }
            gl.window().request_redraw();
        }
    }
}

/// Returns true when the audio device should be paused (cpal callback outputs
/// silence without counting underruns).
///
/// Audio must be paused both when the window loses focus (existing behaviour)
/// and when the emulator is paused in the debugger.  Without the debugger
/// guard, starting with `--debugger` resumes the audio device immediately but
/// never produces samples, causing continuous underrun warnings.
fn audio_should_be_paused(window_focused: bool, debugger_paused: bool) -> bool {
    !window_focused || debugger_paused
}

/// Returns true when frames must be throttled with a manual timer (WaitUntil)
/// rather than relying on vsync to pace the event loop.
///
/// When `vsync_enabled` is false the manual timer is always needed.  When the
/// window loses focus the OS may stop blocking on vsync (especially on macOS),
/// and the audio device is paused, removing the ring-buffer back-pressure that
/// would otherwise limit frame rate.  Using a manual WaitUntil deadline in that
/// case prevents the emulator from running at unconstrained speed.
pub fn should_use_manual_frame_throttle(vsync_enabled: bool, window_focused: bool) -> bool {
    !vsync_enabled || !window_focused
}

/// Returns true when an elapsed-time frame guard is needed to prevent the
/// emulator from running faster than the target hardware frame rate.
///
/// The frame guard is required whenever there is no other mechanism providing
/// back-pressure:
/// - Manual throttle mode (vsync off or window unfocused): WaitUntil provides
///   the deadline but RedrawRequested can arrive early; the guard prevents
///   double-stepping.
/// - Game Boy: the GB has no audio output so there is no ring-buffer
///   back-pressure to pace the event loop in vsync+focused mode.
/// - Audio disabled (`--no-audio`): when the user runs with audio disabled,
///   the NES ring-buffer back-pressure is absent and the emulator would
///   otherwise run uncapped.
fn needs_frame_guard(using_manual_throttle: bool, is_game_boy: bool, audio_disabled: bool) -> bool {
    using_manual_throttle || is_game_boy || audio_disabled
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_throttle_required_when_vsync_enabled_but_unfocused() {
        // Regression: window unfocused → audio paused → no ring-buffer back-pressure.
        // vsync may not block on non-focused windows, so we must throttle manually.
        assert!(
            should_use_manual_frame_throttle(true, false),
            "vsync=true, focused=false must require manual throttle"
        );
    }

    #[test]
    fn manual_throttle_not_required_when_vsync_enabled_and_focused() {
        assert!(
            !should_use_manual_frame_throttle(true, true),
            "vsync=true, focused=true should let vsync drive timing"
        );
    }

    #[test]
    fn manual_throttle_required_when_vsync_disabled() {
        assert!(should_use_manual_frame_throttle(false, true));
        assert!(should_use_manual_frame_throttle(false, false));
    }

    // ── needs_frame_guard (issue #2142) ───────────────────────────────────────

    #[test]
    fn frame_guard_required_when_manual_throttle_active() {
        assert!(
            needs_frame_guard(true, false, false),
            "manual throttle active must require frame guard"
        );
    }

    #[test]
    fn frame_guard_required_for_game_boy() {
        assert!(
            needs_frame_guard(false, true, false),
            "Game Boy has no audio back-pressure, must require frame guard"
        );
    }

    #[test]
    fn frame_guard_required_for_nes_when_audio_disabled() {
        // Regression: when NES is launched with --no-audio, there is no
        // ring-buffer back-pressure, so the emulator would run uncapped
        // without a frame guard.
        assert!(
            needs_frame_guard(false, false, true),
            "NES with audio disabled must require frame guard (issue #2142)"
        );
    }

    #[test]
    fn frame_guard_not_required_for_nes_with_audio_enabled_and_vsync_focused() {
        // Normal NES use: vsync focused + audio enabled → ring-buffer provides
        // back-pressure; no additional frame guard needed.
        assert!(
            !needs_frame_guard(false, false, false),
            "NES with audio enabled and vsync+focused must not require frame guard"
        );
    }

    // ── audio_should_be_paused (#1858) ────────────────────────────────────────

    #[test]
    fn audio_paused_when_debugger_active_and_window_focused() {
        // Starting with --debugger keeps the emulator paused before the first
        // frame.  Audio must also be paused so the cpal callback outputs
        // silence rather than counting underruns.
        assert!(
            audio_should_be_paused(true, true),
            "audio must be paused when debugger is active, even if window is focused"
        );
    }

    #[test]
    fn audio_paused_when_debugger_active_and_window_unfocused() {
        assert!(
            audio_should_be_paused(false, true),
            "audio must be paused when debugger is active and window is unfocused"
        );
    }

    #[test]
    fn audio_paused_when_window_not_focused_and_not_debugging() {
        assert!(
            audio_should_be_paused(false, false),
            "audio must be paused when window lacks focus (existing behaviour)"
        );
    }

    #[test]
    fn audio_plays_when_focused_and_emulator_running() {
        assert!(
            !audio_should_be_paused(true, false),
            "audio must play when window is focused and emulator is running"
        );
    }
}
