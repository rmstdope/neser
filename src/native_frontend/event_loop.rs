//! Native event loop for the NES emulator.
//!
//! Uses winit's `ApplicationHandler` to drive the emulation loop
//! with rendering via `NativeGlWrapper` and audio via `NativeAudio`.

use crate::app_context::SharedAppContext;
use crate::audio::NesAudio;
use crate::autorun::state::AutorunState;
use crate::console::{AutorunMode, Nes, TimingMode};
use crate::debugging::Tracing;
use crate::debugging::control::DebuggerController;
use crate::frontend_toasts::gamepad_init_toast_message;
use crate::native_frontend::app_state::NativeAppState;
use crate::native_frontend::audio::NativeAudio;
use crate::native_frontend::gamepad::GamepadManager;
use crate::native_frontend::gl_wrapper::NativeGlWrapper;
use crate::native_frontend::keyboard::{self, KeyOutcome};
use crate::native_frontend::mouse;
use crate::native_frontend::sleep_inhibitor::SleepInhibitor;

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
    debugger_controller: DebuggerController,
    /// Whether the user had manually paused before the debugger opened,
    /// so we can restore pause state when the debugger closes.
    paused_before_debugger: bool,
    /// Whether the user had manually paused before the cart-switch dialog opened.
    paused_before_cart_switch: bool,
    gamepad: Option<GamepadManager>,
    gamepads_enabled: bool,
    gamepad_toast_shown: bool,
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
}

impl NativeEventLoop {
    pub fn new(
        app_context: SharedAppContext,
        nes: Nes,
        audio: Option<NativeAudio>,
        tracing: Tracing,
        headless: bool,
    ) -> Self {
        let (gamepads_enabled, four_score, fullscreen, vsync_enabled, debugger_controller) = {
            let config = app_context.borrow().config().clone();
            let dc = DebuggerController::new(&config.breakpoints, config.debugger_enabled);
            (
                config.gamepads_enabled,
                config.four_score_enabled,
                config.fullscreen,
                config.vsync_enabled,
                dc,
            )
        };

        let (gamepad, gamepad_init_failed) = if gamepads_enabled {
            match GamepadManager::new(four_score) {
                Ok(gp) => (Some(gp), false),
                Err(e) => {
                    crate::debugging::log_info(format!("Gamepad init failed: {e}"));
                    (None, true)
                }
            }
        } else {
            (None, false)
        };

        let sleep_inhibitor = match SleepInhibitor::new() {
            Ok(si) => Some(si),
            Err(e) => {
                crate::debugging::log_info(format!("Sleep inhibitor init failed: {e}"));
                None
            }
        };

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
            debugger_controller,
            paused_before_debugger: false,
            paused_before_cart_switch: false,
            gamepad,
            gamepads_enabled,
            gamepad_toast_shown: false,
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
        }
    }

    pub fn run(mut self) -> Result<(), String> {
        if self.headless {
            return self.run_headless();
        }
        let event_loop =
            EventLoop::new().map_err(|e| format!("Failed to create event loop: {e}"))?;
        event_loop
            .run_app(&mut self)
            .map_err(|e| format!("Event loop error: {e}"))?;
        // Propagate deferred autorun exit if set during the event loop.
        if let Some(exit_str) = self.autorun_exit.take() {
            return Err(exit_str);
        }
        Ok(())
    }

    fn initialize_audio(&mut self) {
        if let Some(ref mut audio) = self.audio {
            audio.prime_startup(2048);
            audio.resume();
        }
    }

    fn run_frame(&mut self) {
        if self.state.paused && !self.debugger_controller.is_paused() {
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
        self.debugger_controller
            .run_frame(&mut self.nes, &self.tracing, &mut |nes| {
                if let Some(ref mut audio) = *audio_cell.borrow_mut() {
                    while nes.sample_ready() {
                        if let Some(sample) = nes.get_sample() {
                            audio.queue_sample(sample);
                        }
                    }
                }
            });
        self.audio = audio_cell.into_inner();
        self.sync_from_controller();
        self.nes.clear_ready_to_render();

        self.handle_autorun_after_frame(autorun_checkpoint_due);

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

    /// Sync frontend state from the debugger controller.
    fn sync_from_controller(&mut self) {
        if self.debugger_controller.is_debugger_open() {
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
                        mouse::update_mouse_motion(&mut self.nes, cx as i32, cy as i32, w, h);
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
            let catalog_path =
                crate::console::default_catalog_csv_path(std::path::PathBuf::from(home).as_path());
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
        let rom_bytes = match std::fs::read(rom_path) {
            Ok(bytes) => bytes,
            Err(err) => {
                crate::debugging::log_info(format!("Failed to read ROM: {err}"));
                return;
            }
        };

        let app_context = self.nes.app_context().clone();
        let cartridge = match crate::cartridge::Cartridge::load_from_file(
            &rom_bytes,
            rom_path,
            app_context.clone(),
        ) {
            Ok(c) => c,
            Err(err) => {
                crate::debugging::log_info(format!("Failed to load ROM cartridge: {err}"));
                return;
            }
        };

        let applied = {
            let rom_timing = cartridge.rom_timing_mode();
            app_context
                .borrow_mut()
                .config_mut()
                .apply_rom_timing_mode(rom_timing)
        };

        self.nes.insert_cartridge(cartridge);
        crate::console::log_hardware_selection(&app_context, applied);
        self.nes.reset(false);
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
        format: crate::autorun::AutorunFormat,
    ) -> Result<(), String> {
        if mode != AutorunMode::None {
            let (state, pending) =
                AutorunState::new(mode, rom_path, overwrite, extend, from_checkpoint, format)?;
            if let Some(restore) = pending {
                let save_state = crate::console::SaveState::from_bytes(&restore.state_bytes)
                    .map_err(|e| format!("Failed to deserialize checkpoint state: {e}"))?;
                self.nes
                    .load_state(&save_state)
                    .map_err(|e| format!("Failed to restore checkpoint state: {e}"))?;
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
                self.nes.set_joypad_button_states(1, frame.player1);
                self.nes.set_joypad_button_states(2, frame.player2);
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
            let player1 = self.nes.get_joypad_button_states(1);
            let player2 = self.nes.get_joypad_button_states(2);
            return autorun_state.record_frame(player1, player2);
        }
        false
    }

    /// Handle autorun actions after a frame has been fully rendered.
    ///
    /// Captures checkpoints in record mode and verifies CRCs in playback mode.
    fn handle_autorun_after_frame(&mut self, checkpoint_due: bool) {
        if checkpoint_due {
            let crc = self.nes.ppu().borrow().screen_buffer().crc32();
            let state_bytes = self.nes.save_state().to_bytes().unwrap_or_default();
            if let Some(ref mut autorun_state) = self.autorun_state {
                autorun_state.record_checkpoint(crc, state_bytes);
            }
        }

        if let Some(ref mut autorun_state) = self.autorun_state
            && (autorun_state.mode() == AutorunMode::Playback
                || autorun_state.is_extending_playback())
        {
            let crc = self.nes.ppu().borrow().screen_buffer().crc32();
            if let Some(matched) = autorun_state.check_playback_checkpoint(crc) {
                let current_frame = autorun_state.current_frame_index();
                let total_frames = autorun_state.total_frames();
                let current_checkpoint = autorun_state.total_checkpoints_verified();
                let total_checkpoints = autorun_state.total_checkpoints();
                if matched {
                    crate::debugging::log_info(format!(
                        "Autorun checkpoint CRC match (0x{crc:08X}) at frame {current_frame}/{total_frames}, checkpoint {current_checkpoint}/{total_checkpoints}",
                    ));
                } else {
                    crate::debugging::log_info(format!(
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
        let crc = self.nes.ppu().borrow().screen_buffer().crc32();

        if mismatches == 0 {
            crate::debugging::log_info(format!(
                "Autorun playback successful: {verified} checkpoints verified, final CRC 0x{crc:08X}",
            ));
            Err("AUTORUN_EXIT:0".to_string())
        } else {
            crate::debugging::log_info(format!(
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

        let crc = self.nes.ppu().borrow().screen_buffer().crc32();
        let state_bytes = self.nes.save_state().to_bytes().unwrap_or_default();

        autorun_state.save_with_final_checkpoint(crc, state_bytes)?;
        crate::debugging::log_info(format!(
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

            crate::autorun::headless_playback::run_one_frame(&mut self.nes);

            self.handle_autorun_after_frame(checkpoint_due);
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
                    self.debugger_controller
                        .load_breakpoints_from_debug_file(&self.nes);
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
                self.debugger_controller
                    .save_breakpoints_to_debug_file(&self.nes);
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
                if focused {
                    if let Some(ref audio) = self.audio {
                        audio.resume();
                    }
                } else {
                    if let Some(ref audio) = self.audio {
                        audio.pause();
                    }
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
                        KeyOutcome::Quit => {
                            if let Err(e) = self.finish_recording() {
                                eprintln!("Failed to finish recording on quit: {e}");
                            }
                            self.debugger_controller
                                .save_breakpoints_to_debug_file(&self.nes);
                            event_loop.exit();
                        }
                        KeyOutcome::CycleShader => {
                            if let Some(ref mut gl) = self.gl_wrapper {
                                gl.cycle_shader();
                            }
                        }
                        KeyOutcome::ToggleDebugger => {
                            self.debugger_controller.toggle_debugger(&self.nes);
                            self.sync_from_controller();
                        }
                        KeyOutcome::StepOver => {
                            self.debugger_controller.step_over(&mut self.nes);
                            self.sync_from_controller();
                        }
                        KeyOutcome::StepInto => {
                            self.debugger_controller.step_into(&mut self.nes);
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
                    }
                } else {
                    keyboard::handle_key_released(&mut self.nes, key_code);
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
                // Forward to imgui/UI layer always.
                if let Some(ref mut gl) = self.gl_wrapper {
                    gl.handle_cursor_moved(position);
                }
                // When grabbed, all position input comes via DeviceEvent::MouseMotion
                // (accumulated into virtual_cursor). CursorMoved is unreliable in
                // Locked grab mode — the reported position is always the lock point.
            }

            WindowEvent::MouseInput { button, state, .. } => {
                // Forward to imgui/UI layer.
                if let Some(ref mut gl) = self.gl_wrapper {
                    gl.handle_mouse_button(button, state);
                }

                let has_mouse = mouse::has_any_mouse_controller(&self.nes);

                // Left-click grabs immediately so the same click is also forwarded
                // as a button press (unlike deferring to the next frame, which
                // would swallow Zapper shots and Arkanoid trigger presses).
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
                        if let Some(ref mut gl) = self.gl_wrapper {
                            let _ = gl.set_mouse_grab_locked();
                            gl.window().set_cursor_visible(false);
                            // Centre virtual cursor and immediately sync NES coords.
                            let (w, h) = gl.window_size();
                            let cx = w as f32 / 2.0;
                            let cy = h as f32 / 2.0;
                            self.state.virtual_cursor = (cx, cy);
                            self.state.last_zapper_position = mouse::update_mouse_motion(
                                &mut self.nes,
                                cx as i32,
                                cy as i32,
                                w,
                                h,
                            );
                        }
                        self.state.mouse_grabbed = true;
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
                // If autorun signalled exit during the last frame, exit now.
                if self.autorun_exit.is_some() {
                    event_loop.exit();
                    return;
                }

                // Run one frame of emulation
                self.run_frame();

                // If autorun signalled exit during this frame, exit now.
                if self.autorun_exit.is_some() {
                    event_loop.exit();
                    return;
                }

                // Sync mouse grab state each frame.
                self.sync_mouse_grab_state();

                // Render and apply debugger UI actions
                let action = if let Some(ref mut gl) = self.gl_wrapper {
                    gl.update_breakpoints(self.debugger_controller.breakpoints());
                    let overlay = self
                        .state
                        .overlay_text(&self.nes, self.autorun_state.as_ref());
                    let crosshair =
                        mouse::zapper_crosshair(&self.nes, self.state.last_zapper_position);
                    gl.render(
                        &self.nes,
                        self.state.debugger_open,
                        overlay.as_deref(),
                        false,
                        crosshair,
                    )
                } else {
                    Default::default()
                };
                self.debugger_controller
                    .apply_ui_action(&mut self.nes, action);
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

            if self.nes.has_snes_mouse() && !mouse::has_zapper(&self.nes) {
                // SNES Mouse: pass raw deltas directly.
                // Zapper takes precedence — if a Zapper is also connected,
                // fall through to the virtual-cursor path (matching SDL logic).
                mouse::apply_snes_mouse_relative_motion(&mut self.nes, dx as i32, dy as i32, w, h);
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

                self.state.last_zapper_position =
                    mouse::update_mouse_motion(&mut self.nes, new_vx as i32, new_vy as i32, w, h);
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Poll gamepad events before requesting redraw.
        if let Some(ref mut gp) = self.gamepad {
            gp.process_events(&mut self.nes);
        }

        // Show the gamepad toast after the first process_events() call.
        // On macOS, IOKit enumerates gamepads asynchronously so Connected
        // events aren't available until the event loop is running.
        if !self.gamepad_toast_shown {
            self.gamepad_toast_shown = true;
            let toast = if self.gamepad_init_failed {
                "Gamepad init failed: using keyboard controls".to_string()
            } else {
                let count = self.gamepad.as_ref().map_or(0, |g| g.connected_count());
                gamepad_init_toast_message(self.gamepads_enabled, count)
            };
            self.app_context.borrow_mut().add_toast(&toast);
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
            } else if !self.vsync_enabled {
                // Manual frame limiting: advance deadline by target interval.
                let timing_mode = self
                    .nes
                    .app_context()
                    .borrow()
                    .config()
                    .hardware_model
                    .timing_mode();
                let target = target_frame_duration(timing_mode);
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

/// Returns the target duration per frame for the given timing mode.
fn target_frame_duration(timing_mode: TimingMode) -> Duration {
    Duration::from_secs_f64(1.0 / timing_mode.frame_rate_hz())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_target_frame_duration_ntsc() {
        let duration = target_frame_duration(TimingMode::Ntsc);
        // NTSC: ~60.098 FPS → ~16.64ms per frame
        let ms = duration.as_secs_f64() * 1000.0;
        assert!(
            (16.0..=17.0).contains(&ms),
            "NTSC frame duration should be ~16.6ms, got {ms:.2}ms"
        );
    }

    #[test]
    fn test_target_frame_duration_pal() {
        let duration = target_frame_duration(TimingMode::Pal);
        // PAL: ~50.007 FPS → ~20.0ms per frame
        let ms = duration.as_secs_f64() * 1000.0;
        assert!(
            (19.5..=20.5).contains(&ms),
            "PAL frame duration should be ~20.0ms, got {ms:.2}ms"
        );
    }
}
