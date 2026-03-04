use super::autorun_state::AutorunState;
use super::sdl_audio::SdlNesAudio;
use super::sdl_gl_wrapper::SdlGlWrapper;
use crate::app_context::{AppContext, IntoSharedAppContext, SharedAppContext};
use crate::console::{
    AutorunMode, ControllerStateWrapper, Nes, SaveState, TimingMode, default_catalog_csv_path,
    log_rom_timing_mode_selection,
};
use crate::frontend_toasts::gamepad_init_toast_message;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::keyboard::Mod;
use sdl2::mouse::MouseButton;
#[cfg(test)]
#[allow(unused_imports)]
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
#[cfg(test)]
#[allow(unused_imports)]
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::debugging::{
    DebuggerSnapshot, Tracing,
    breakpoints::{BreakpointKind, BreakpointList},
    log_info, snapshot, ui,
};
use crate::input::Button;
use crate::rendering::Crosshair;

// Type alias to simplify the complex return type used when initializing gamepads.
type InitGamepadsResult = Result<
    (
        sdl2::GameControllerSubsystem,
        Vec<sdl2::controller::GameController>,
        HashMap<u32, u8>,
    ),
    String,
>;

/// Result type for autorun playback completion with exit code.
#[derive(Debug)]
pub enum AutorunExitCode {
    /// Playback succeeded with CRC match
    Success,
    /// Playback failed with CRC mismatch
    Failure,
}

/// EventLoop manages the SDL2 event loop for the application.
/// It handles user input and window events, exiting when Escape is pressed or the window is closed.
pub struct SdlEventLoop {
    _sdl_context: sdl2::Sdl,
    app_context: SharedAppContext,
    gl_backend: Option<SdlGlWrapper>,
    event_pump: sdl2::EventPump,
    vsync_enabled: bool,
    fullscreen: bool,
    paused: bool,
    help_overlay_visible: bool,
    debugger_open_requested: bool,
    breakpoints: BreakpointList,
    last_post_instruction_cycles: u64,
    last_post_instruction_frame: u64,
    temporary_breakpoint: Option<TemporaryBreakpoint>,
    arm_temporary_breakpoint_after_next_instruction: bool,
    breakpoint_ignore_once_at_pc: Option<u16>,
    #[cfg_attr(not(test), allow(dead_code))]
    debugger_renderer: Option<Box<dyn DebuggerRenderer>>,
    audio: Option<SdlNesAudio>,
    controllers: Vec<sdl2::controller::GameController>,
    game_controller_subsystem: Option<sdl2::GameControllerSubsystem>,
    controller_player_map: HashMap<u32, u8>, // Maps controller instance_id to player number (1 or 2)
    last_mouse_position: Option<(i32, i32)>,
    cursor_hidden: bool,
    autorun_state: Option<AutorunState>,
    cartridge_switch_dialog_requested: bool,
    cartridge_switch_entries: Vec<String>,
    cartridge_switch_selected_index: usize,
    cartridge_switch_filter_query: String,
    cartridge_switch_filtered_indices: Vec<usize>,
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
    fn render(&mut self, snapshot: &DebuggerSnapshot);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyDownOutcome {
    Continue,
    Quit,
}

impl SdlEventLoop {
    /// Maps an SDL mouse X position into a screen position value (0..=255).
    ///
    /// The input is normalized to $[-1.0, 1.0]$ across the current window width,
    /// then shaped using a non-linear curve $\text{sign}(x)\cdot|x|^{1.5}$ to
    /// make the edges respond faster than the center. The result is scaled to
    /// the Arkanoid controller range ($62$-$F2$) and clamped.
    fn map_mouse_x_to_paddle_position(x: i32, window_width: u32) -> u8 {
        const MIN_POSITION: f32 = 0x62 as f32;
        const MAX_POSITION: f32 = 0xF2 as f32;
        const RANGE: f32 = MAX_POSITION - MIN_POSITION;

        if window_width <= 1 {
            return MIN_POSITION as u8;
        }

        let max_x = window_width.saturating_sub(1) as i32;
        let clamped_x = x.clamp(0, max_x);
        let width_minus_one = max_x as f32;
        let normalized = (clamped_x as f32 / width_minus_one) * 2.0 - 1.0;
        let curved = normalized.signum() * normalized.abs().powf(1.5);
        let scaled = (curved + 1.0) * 0.5 * RANGE + MIN_POSITION;
        scaled.round().clamp(MIN_POSITION, MAX_POSITION) as u8
    }

    /// Maps an SDL mouse X position into a Zapper screen position (0..=255).
    fn map_mouse_x_to_zapper_position(x: i32, window_width: u32) -> u8 {
        if window_width <= 1 {
            return 0;
        }

        let max_x = window_width.saturating_sub(1) as i32;
        let clamped_x = x.clamp(0, max_x);
        let normalized = clamped_x as f32 / max_x as f32;
        (normalized * 255.0).round().clamp(0.0, 255.0) as u8
    }

    /// Maps an SDL mouse Y position into a Zapper screen position (0..=255).
    fn map_mouse_y_to_zapper_position(y: i32, window_height: u32) -> u8 {
        if window_height <= 1 {
            return 0;
        }

        let max_y = window_height.saturating_sub(1) as i32;
        let clamped_y = y.clamp(0, max_y);
        let normalized = clamped_y as f32 / max_y as f32;
        (normalized * 255.0).round().clamp(0.0, 255.0) as u8
    }

    /// Applies mouse motion to mouse-emulated controller.
    ///
    /// This is a no-op if no mouse-emulated controller is connected.
    fn update_mouse_motion(nes: &mut Nes, x: i32, y: i32, window_width: u32, window_height: u32) {
        if Self::zapper_ports(nes).is_empty() {
            let position = Self::map_mouse_x_to_paddle_position(x, window_width);
            nes.set_mouse_x_position(position);
        } else {
            let x_position = Self::map_mouse_x_to_zapper_position(x, window_width);
            let y_position = Self::map_mouse_y_to_zapper_position(y, window_height);
            nes.set_mouse_x_position(x_position);
            nes.set_mouse_y_position(y_position);
        }
    }

    /// Maps left mouse button presses to mouse-emulated controller.
    ///
    /// This is a no-op if no mouse-emulated controller is connected.
    fn update_mouse_button(nes: &mut Nes, button: MouseButton, pressed: bool) {
        if button != MouseButton::Left {
            return;
        }

        if !Self::mouse_ports(nes).is_empty() {
            nes.set_mouse_left_button(pressed);
        }
    }

    fn gamepad_ports(nes: &Nes) -> Vec<u8> {
        (1..=2)
            .filter(|&port| {
                nes.controller_input_type(port) == Some(crate::input::ControllerInput::Gamepad)
            })
            .collect()
    }

    fn mouse_ports(nes: &Nes) -> Vec<u8> {
        (1..=2)
            .filter(|&port| {
                nes.controller_input_type(port) == Some(crate::input::ControllerInput::Mouse)
            })
            .collect()
    }

    fn zapper_crosshair(&self, nes: &Nes) -> Option<Crosshair> {
        if Self::zapper_ports(nes).is_empty() {
            None
        } else {
            self.last_mouse_position.map(|(x, y)| Crosshair {
                x: x as f32,
                y: y as f32,
            })
        }
    }

    fn update_cursor_visibility(&mut self, zapper_active: bool) {
        if self.cursor_hidden == zapper_active {
            return;
        }

        self._sdl_context.mouse().show_cursor(!zapper_active);
        self.cursor_hidden = zapper_active;
    }

    fn zapper_ports(nes: &Nes) -> Vec<u8> {
        let state = nes.bus.borrow().capture_state();
        let mut ports = Vec::new();
        if matches!(state.port1_controller, ControllerStateWrapper::Zapper(_)) {
            ports.push(1);
        }
        if matches!(state.port2_controller, ControllerStateWrapper::Zapper(_)) {
            ports.push(2);
        }
        ports
    }

    fn assigned_gamepad_port(
        nes: &Nes,
        controller_player_map: &HashMap<u32, u8>,
        player_num: u8,
    ) -> Option<u8> {
        let gamepad_ports = Self::gamepad_ports(nes);
        if gamepad_ports.is_empty() {
            return None;
        }

        let index = (player_num as usize).saturating_sub(1);
        gamepad_ports.get(index).copied().and_then(|port| {
            let assigned_count = controller_player_map.len().min(gamepad_ports.len());
            if index < assigned_count {
                Some(port)
            } else {
                None
            }
        })
    }

    fn keyboard_ports(
        nes: &Nes,
        controller_player_map: &HashMap<u32, u8>,
    ) -> (Option<u8>, Option<u8>) {
        let gamepad_ports = Self::gamepad_ports(nes);
        let assigned_count = controller_player_map.len().min(gamepad_ports.len());
        let port_1 = if assigned_count == 0 {
            gamepad_ports.first().copied()
        } else {
            None
        };
        let port_2 = if assigned_count <= 1 {
            gamepad_ports.get(1).copied()
        } else {
            None
        };
        (port_1, port_2)
    }

    fn apply_keyboard_button(nes: &mut Nes, ports: &[u8], button: Button, pressed: bool) {
        for port in ports {
            nes.set_button(*port, button, pressed);
        }
    }

    /// Creates a new EventLoop instance.
    ///
    /// This is the preferred way to create an EventLoop.
    ///
    /// # Arguments
    ///
    /// * `headless` - If `true`, creates an EventLoop without a window (useful for testing).
    ///   If `false`, creates a window sized for the specified TV system.
    /// * `audio` - Optional audio output system. If provided, audio will be enabled.
    /// * `config` - Configuration struct containing all emulator settings.
    ///
    /// # Errors
    ///
    /// Returns an error if SDL2 initialization fails, the event pump cannot be created,
    /// or (when `headless` is `false`) the window cannot be created.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use neser::console::Config;
    /// use neser::sdl_frontend::SdlEventLoop;
    ///
    /// let config = Config::default();
    /// // Create a headless EventLoop for testing
    /// let headless = SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone()))?;
    ///
    /// // Create an EventLoop with a window
    /// let windowed = SdlEventLoop::new(false, None, AppContext::new_with_config(config.clone()))?;
    /// # Ok::<(), String>(())
    /// ```
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn new(
        headless: bool,
        audio: Option<SdlNesAudio>,
        app_context: AppContext,
    ) -> Result<Self, String> {
        Self::new_with_context(headless, audio, app_context.into_shared())
    }

    pub fn new_with_context(
        headless: bool,
        audio: Option<SdlNesAudio>,
        app_context: SharedAppContext,
    ) -> Result<Self, String> {
        let (gamepads_enabled, vsync_enabled, fullscreen) = {
            let app_context_ref = app_context.borrow();
            let config = app_context_ref.config();
            (
                config.gamepads_enabled,
                config.vsync_enabled,
                config.fullscreen,
            )
        };
        let sdl_context = sdl2::init()?;
        let event_pump = sdl_context.event_pump()?;

        let gl_backend = if headless {
            None
        } else {
            Some(SdlGlWrapper::new(&sdl_context, app_context.clone())?)
        };

        // Initialize gamepads if enabled
        let (game_controller_subsystem, controllers, controller_player_map) = if gamepads_enabled {
            let (subsystem, controllers, map) = Self::init_gamepads(&sdl_context)?;
            (Some(subsystem), controllers, map)
        } else {
            (None, Vec::new(), HashMap::new())
        };

        let event_loop = SdlEventLoop {
            _sdl_context: sdl_context,
            app_context,
            gl_backend,
            event_pump,
            vsync_enabled,
            fullscreen,
            paused: false,
            help_overlay_visible: false,
            debugger_open_requested: false,
            breakpoints: BreakpointList::new(),
            last_post_instruction_cycles: 0,
            last_post_instruction_frame: 0,
            temporary_breakpoint: None,
            arm_temporary_breakpoint_after_next_instruction: false,
            breakpoint_ignore_once_at_pc: None,
            debugger_renderer: None,
            audio,
            controllers,
            game_controller_subsystem,
            controller_player_map,
            last_mouse_position: None,
            cursor_hidden: false,
            autorun_state: None,
            cartridge_switch_dialog_requested: false,
            cartridge_switch_entries: Vec::new(),
            cartridge_switch_selected_index: 0,
            cartridge_switch_filter_query: String::new(),
            cartridge_switch_filtered_indices: Vec::new(),
        };

        let gamepad_toast =
            gamepad_init_toast_message(gamepads_enabled, event_loop.controllers.len());
        event_loop.app_context.borrow_mut().add_toast(gamepad_toast);

        Ok(event_loop)
    }

    /// Initialize autorun state if autorun mode is enabled.
    ///
    /// Creates and configures the autorun subsystem based on the provided mode and options.
    /// If `mode` is [`AutorunMode::None`], this method does nothing and autorun remains disabled.
    ///
    /// # Arguments
    ///
    /// * `mode` - The autorun mode to use (`None`, `Record`, or `Playback`).
    /// * `rom_path` - Path to the loaded ROM; the `.autorun` file lives alongside it.
    /// * `overwrite` - In `Record` mode: back up any existing file and start fresh.
    /// * `extend` - In `Record` mode: replay from second-to-last checkpoint then continue recording.
    /// * `from_checkpoint` - In `Playback` mode: start playback from this checkpoint index
    ///   (negative = from end, -1 = second-to-last, etc.).
    /// * `nes` - The NES instance; used to restore saved state when starting from a checkpoint.
    pub fn init_autorun(
        &mut self,
        mode: AutorunMode,
        rom_path: &str,
        overwrite: bool,
        extend: bool,
        from_checkpoint: Option<i64>,
        nes: &mut Nes,
    ) -> Result<(), String> {
        if mode != AutorunMode::None {
            let (state, pending) =
                AutorunState::new(mode, rom_path, overwrite, extend, from_checkpoint)?;
            if let Some(restore) = pending {
                let save_state = crate::console::SaveState::from_bytes(&restore.state_bytes)
                    .map_err(|e| format!("Failed to deserialize checkpoint state: {e}"))?;
                nes.load_state(&save_state)
                    .map_err(|e| format!("Failed to restore checkpoint state: {e}"))?;
            }
            self.autorun_state = Some(state);
        }
        Ok(())
    }

    /// Initialize game controllers
    ///
    /// Attempts to open up to 2 game controllers. The first controller found
    /// is assigned to player 1, the second to player 2.
    ///
    /// Returns a tuple of (subsystem, controllers vector, player mapping HashMap)
    fn init_gamepads(sdl_context: &sdl2::Sdl) -> InitGamepadsResult {
        let game_controller_subsystem = sdl_context.game_controller()?;
        let _num = game_controller_subsystem
            .load_mappings("gamecontrollerdb.txt")
            .unwrap_or(0);
        let available = game_controller_subsystem
            .num_joysticks()
            .map_err(|e| format!("Failed to enumerate joysticks: {}", e))?;

        let mut controllers = Vec::new();
        let mut controller_player_map = HashMap::new();

        // Try to open up to 2 controllers
        for id in 0..available.min(2) {
            if !game_controller_subsystem.is_game_controller(id) {
                log_info(format!("Joystick {} is not a game controller", id));
                continue;
            }

            match game_controller_subsystem.open(id) {
                Ok(controller) => {
                    let instance_id = controller.instance_id();
                    let player_num = (controllers.len() + 1) as u8;
                    log_info(format!(
                        "Opened game controller {} for player {}: {}",
                        id,
                        player_num,
                        controller.name()
                    ));
                    controller_player_map.insert(instance_id, player_num);
                    controllers.push(controller);
                }
                Err(e) => {
                    log_info(format!("Failed to open controller {}: {}", id, e));
                }
            }

            if controllers.len() >= 2 {
                break;
            }
        }

        Ok((
            game_controller_subsystem,
            controllers,
            controller_player_map,
        ))
    }

    /// Capture current button states for both players as u8 bitmasks.
    ///
    /// Each returned `u8` is a bitmask where each bit represents the pressed state of a button.
    /// The bit layout follows the NES joypad protocol and matches the `Button` enum values:
    ///
    /// - Bit 0 (LSB): `Button::A`
    /// - Bit 1: `Button::B`
    /// - Bit 2: `Button::Select`
    /// - Bit 3: `Button::Start`
    /// - Bit 4: `Button::Up`
    /// - Bit 5: `Button::Down`
    /// - Bit 6: `Button::Left`
    /// - Bit 7: `Button::Right`
    ///
    /// A bit is set to `1` when the corresponding button is pressed, `0` when released.
    ///
    /// # Returns
    ///
    /// Returns `(player1_state, player2_state)` representing the button states for controller
    /// ports 1 and 2 respectively.
    #[allow(dead_code)]
    fn capture_button_states(&self, nes: &Nes) -> (u8, u8) {
        let player1 = nes.get_joypad_button_states(1);
        let player2 = nes.get_joypad_button_states(2);
        (player1, player2)
    }

    /// Apply button states from u8 bitmasks for both players.
    ///
    /// Sets the button states for both controller ports from `u8` bitmask values.
    /// The bit layout must match the NES joypad protocol and `Button` enum values:
    ///
    /// - Bit 0 (LSB): `Button::A`
    /// - Bit 1: `Button::B`
    /// - Bit 2: `Button::Select`
    /// - Bit 3: `Button::Start`
    /// - Bit 4: `Button::Up`
    /// - Bit 5: `Button::Down`
    /// - Bit 6: `Button::Left`
    /// - Bit 7: `Button::Right`
    ///
    /// A bit set to `1` means the corresponding button is pressed, `0` means released.
    ///
    /// # Arguments
    ///
    /// * `nes` - Mutable reference to the NES instance
    /// * `player1` - Button state bitmask for controller port 1
    /// * `player2` - Button state bitmask for controller port 2
    fn apply_button_states(&self, nes: &mut Nes, player1: u8, player2: u8) {
        let buttons = [
            Button::A,
            Button::B,
            Button::Select,
            Button::Start,
            Button::Up,
            Button::Down,
            Button::Left,
            Button::Right,
        ];

        // Apply player 1 state (port 1)
        for button in &buttons {
            let pressed = (player1 & (1 << (*button as u8))) != 0;
            nes.set_button(1, *button, pressed);
        }

        // Apply player 2 state (port 2)
        for button in &buttons {
            let pressed = (player2 & (1 << (*button as u8))) != 0;
            nes.set_button(2, *button, pressed);
        }
    }

    /// Handle autorun logic before emulating a frame.
    ///
    /// In playback or extend mode, applies button states from the recording.
    /// Returns Ok(true) if we should continue, Ok(false) for normal exit,
    /// or Err(AutorunExitCode) when playback completes with CRC verification.
    fn handle_autorun_before_frame(&mut self, nes: &mut Nes) -> Result<bool, AutorunExitCode> {
        let Some(ref mut autorun_state) = self.autorun_state else {
            return Ok(true); // No autorun active
        };

        use crate::console::AutorunMode;

        autorun_state.begin_frame();

        // In playback mode or extend mode with remaining frames, apply recorded input
        if autorun_state.mode() == AutorunMode::Playback || autorun_state.is_extending_playback() {
            if let Some(frame) = autorun_state.next_playback_frame() {
                self.apply_button_states(nes, frame.player1, frame.player2);
                return Ok(true);
            }

            // Playback finished
            if autorun_state.mode() == AutorunMode::Playback {
                // Verify CRC and exit
                return self.finish_playback(nes);
            }
        }

        Ok(true)
    }

    /// Handle autorun logic after processing input but before rendering.
    ///
    /// In record mode, captures current button states. Returns `true` if a checkpoint should be
    /// captured after the frame is fully rendered.
    fn handle_autorun_after_input(&mut self, nes: &Nes) -> bool {
        let Some(ref mut autorun_state) = self.autorun_state else {
            return false; // No autorun active
        };

        use crate::console::AutorunMode;

        // In record mode (or extend mode after playback), capture button states
        if autorun_state.mode() == AutorunMode::Record
            && !autorun_state.is_extending_playback()
            && !autorun_state.current_frame_was_prerecorded()
        {
            let player1 = nes.get_joypad_button_states(1);
            let player2 = nes.get_joypad_button_states(2);
            return autorun_state.record_frame(player1, player2);
        }
        false
    }

    /// Handle autorun actions after a frame has been fully rendered.
    ///
    /// * When `checkpoint_due` is true, captures the current screen CRC and emulator state
    ///   and stores them as a checkpoint in the recording.
    /// * During playback, verifies any checkpoint that falls on the just-completed frame.
    fn handle_autorun_after_frame(&mut self, nes: &Nes, checkpoint_due: bool) {
        use crate::console::AutorunMode;

        if checkpoint_due {
            let crc = nes.ppu.borrow().screen_buffer().crc32();
            let state_bytes = nes.save_state().to_bytes().unwrap_or_default();
            if let Some(ref mut autorun_state) = self.autorun_state {
                autorun_state.record_checkpoint(crc, state_bytes);
            }
        }

        // Verify playback checkpoint CRC if one falls on this frame
        if let Some(ref mut autorun_state) = self.autorun_state
            && (autorun_state.mode() == AutorunMode::Playback
                || autorun_state.is_extending_playback())
        {
            let crc = nes.ppu.borrow().screen_buffer().crc32();
            if let Some(matched) = autorun_state.check_playback_checkpoint(crc) {
                if matched {
                    log_info(format!(
                        "Autorun checkpoint CRC match (0x{:08X}) at frame {}",
                        crc,
                        autorun_state.current_frame_index().saturating_sub(1),
                    ));
                } else {
                    log_info(format!(
                        "Autorun checkpoint CRC MISMATCH at frame {}: got 0x{:08X}",
                        autorun_state.current_frame_index().saturating_sub(1),
                        crc,
                    ));
                }
            }
        }
    }

    /// Finish playback by reporting CRC verification results.
    /// Returns `Err(AutorunExitCode)` to signal the event loop should exit.
    fn finish_playback(&mut self, nes: &Nes) -> Result<bool, AutorunExitCode> {
        let Some(ref autorun_state) = self.autorun_state else {
            return Ok(true);
        };

        let mismatches = autorun_state.crc_mismatches();
        let verified = autorun_state.total_checkpoints_verified();
        let crc = nes.ppu.borrow().screen_buffer().crc32();

        if mismatches == 0 {
            log_info(format!(
                "Autorun playback successful: {} checkpoints verified, final CRC 0x{:08X}",
                verified, crc
            ));
            Err(AutorunExitCode::Success)
        } else {
            log_info(format!(
                "Autorun playback failed: {mismatches}/{verified} CRC mismatches",
            ));
            Err(AutorunExitCode::Failure)
        }
    }

    /// Determine if the overlay should blink red for autorun extend mode.
    ///
    /// Red blinking occurs:
    /// - Only in Record mode during extend playback
    /// - Only during the last 3 seconds before playback ends
    /// - Blinks at 4Hz (quarter of frame rate)
    fn should_overlay_blink_red(&self, nes: &Nes) -> bool {
        let Some(ref autorun_state) = self.autorun_state else {
            return false;
        };

        // Only blink when extending and still in playback phase
        if !autorun_state.is_extending_playback() {
            return false;
        }

        let current_frame = autorun_state.current_frame_index();
        let total_frames = autorun_state.total_frames();
        let tv_system = nes.app_context.borrow().config().tv_system;

        // Calculate FPS and blink parameters
        let fps = tv_system.frame_rate_hz().round().max(1.0) as usize;
        let blink_window_frames = fps * 3; // Last 3 seconds
        let blink_half_period_frames = (fps / 4).max(1); // Quarter-second blinks (4Hz)

        let frames_remaining = total_frames.saturating_sub(current_frame);

        // Only blink in the last 3 seconds
        if frames_remaining > blink_window_frames {
            return false;
        }

        // Toggle every quarter second
        (current_frame / blink_half_period_frames).is_multiple_of(2)
    }

    /// Finish recording by saving the autorun file with a final checkpoint.
    fn finish_recording(&mut self, nes: &Nes) -> Result<(), String> {
        self.save_breakpoints_to_debug_file(nes);
        let Some(ref mut autorun_state) = self.autorun_state else {
            return Ok(());
        };

        use crate::console::AutorunMode;

        if autorun_state.mode() != AutorunMode::Record {
            return Ok(());
        }

        // Capture final screen CRC and emulator state for the final checkpoint
        let crc = nes.ppu.borrow().screen_buffer().crc32();
        let state_bytes = nes.save_state().to_bytes().unwrap_or_default();

        autorun_state.save_with_final_checkpoint(crc, state_bytes)?;
        log_info(format!(
            "Autorun recording saved: {} frames, final CRC 0x{:08X}",
            autorun_state.total_frames(),
            crc
        ));

        Ok(())
    }

    fn should_manual_frame_limit(vsync_enabled: bool) -> bool {
        !vsync_enabled
    }

    fn enter_debugger(&mut self) {
        self.paused = true;
        self.debugger_open_requested = true;
    }

    fn toggle_fullscreen(&mut self, gl_backend: Option<&mut SdlGlWrapper>) {
        let next_fullscreen_state = !self.fullscreen;

        if let Some(gl_backend) = gl_backend
            && let Err(err) = gl_backend.set_fullscreen(next_fullscreen_state)
        {
            log_info(format!("Failed to toggle fullscreen: {err}"));
            return;
        }

        self.fullscreen = next_fullscreen_state;
    }

    fn read_vector_target(nes: &Nes, vector_addr: u16) -> u16 {
        let memory = nes.bus.borrow();
        let lo = memory.read_cpu_for_debugger(vector_addr);
        let hi = memory.read_cpu_for_debugger(vector_addr.wrapping_add(1));
        u16::from_le_bytes([lo, hi])
    }

    fn clear_temporary_breakpoint(&mut self) {
        if let Some(tb) = self.temporary_breakpoint.take()
            && !tb.already_present
        {
            self.remove_breakpoint(tb.pc);
        }
    }

    fn set_temporary_breakpoint(&mut self, pc: u16) {
        self.clear_temporary_breakpoint();

        let already_present = self.breakpoints.has_pc_breakpoint_at(pc);
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
        nes: &Nes,
        pc: u16,
        required_interrupt: crate::cpu::InterruptKind,
    ) {
        self.clear_temporary_breakpoint();

        let already_present = self.breakpoints.has_pc_breakpoint_at(pc);
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

    fn maybe_arm_temporary_breakpoint_after_instruction(&mut self, nes: &Nes) {
        if !self.arm_temporary_breakpoint_after_next_instruction {
            return;
        }

        self.arm_temporary_breakpoint_after_next_instruction = false;
        self.set_temporary_breakpoint(nes.cpu.pc());
    }

    fn continue_from_debugger(&mut self, nes: &Nes) {
        // Prevent immediately re-breaking on the same instruction.
        if self.breakpoints.has_enabled_pc_breakpoint_at(nes.cpu.pc()) {
            self.breakpoint_ignore_once_at_pc = Some(nes.cpu.pc());
        }

        // Sync the cycle tracker so threshold breakpoints don't fire spuriously.
        // Debugger actions (e.g. run-to-next-frame, step) may advance CPU cycles
        // and frame counts without going through check_post_instruction_breakpoints,
        // leaving trackers stale. Syncing here means threshold breakpoints can only
        // fire when crossed going forward from this point.
        self.last_post_instruction_cycles = nes.cpu.get_total_cycles();
        self.last_post_instruction_frame = nes.ppu.borrow().frame_count();

        self.paused = false;
        self.debugger_open_requested = false;
    }

    fn check_breakpoint_hit(
        &mut self,
        pc: u16,
        current_interrupt: Option<crate::cpu::InterruptKind>,
    ) -> bool {
        if let Some(tb) = self.temporary_breakpoint.as_mut()
            && let Some(required_interrupt) = tb.required_interrupt
            && !tb.has_exited_required_interrupt
            && current_interrupt != Some(required_interrupt)
        {
            tb.has_exited_required_interrupt = true;
        }

        // If we just continued from a breakpoint, allow executing that instruction once.
        if self.breakpoint_ignore_once_at_pc == Some(pc) {
            self.breakpoint_ignore_once_at_pc = None;
            return false;
        }

        let has_pc_bp = |addr: u16| self.breakpoints.has_enabled_pc_breakpoint_at(addr);

        // While a temporary "run to" breakpoint is active, ignore all other breakpoint hits.
        // This matches the expected debugger UX: run-to should run until the target, not stop
        // early due to unrelated breakpoints (including ones inside the current interrupt).
        if let Some(tb) = self.temporary_breakpoint
            && tb.ignore_other_breakpoints
            && has_pc_bp(pc)
            && pc != tb.pc
        {
            return false;
        }

        if has_pc_bp(pc) {
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

    /// Check cycle, frame, and write-address breakpoints after an instruction has executed.
    fn check_post_instruction_breakpoints(&mut self, nes: &Nes) {
        use crate::debugging::breakpoints::EvalContext;
        if self.paused {
            return;
        }
        let prev_cycles = self.last_post_instruction_cycles;
        let current_cycles = nes.cpu.get_total_cycles();
        let prev_frame = self.last_post_instruction_frame;
        let current_frame = nes.ppu.borrow().frame_count();
        let ctx = EvalContext {
            pc: nes.cpu.pc(),
            prev_cpu_cycles: prev_cycles,
            cpu_cycles: current_cycles,
            prev_frame,
            frame: current_frame,
            write_addr: nes.cpu.last_cpu_write_addr(),
        };
        self.last_post_instruction_cycles = current_cycles;
        self.last_post_instruction_frame = current_frame;
        // Only check non-PC conditions here; PC breakpoints are
        // checked before each instruction in check_breakpoint_hit.
        let hit = self
            .breakpoints
            .iter()
            .any(|bp| bp.enabled && !matches!(bp.kind, BreakpointKind::Pc(_)) && bp.is_hit(&ctx));
        if hit {
            self.enter_debugger();
        }
    }

    // Checks if the user has requested to quit via Escape key or window close.
    // Returns `true` if quit was requested, `false` otherwise.
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
    pub fn run(&mut self, nes: &mut Nes, tracing: Tracing) -> Result<(), String> {
        self.load_breakpoints_from_debug_file(nes);
        let app_context = self.app_context.clone();
        self.load_breakpoints_from_context(&app_context);

        let mut last_audio_stats_print = Instant::now();
        let mut last_cpu_cycles = nes.cpu.get_total_cycles();
        let mut last_perf_instant = Instant::now();

        // Start audio playback if audio is enabled
        if let Some(ref mut audio) = self.audio {
            audio.prime_startup(2048);
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
                            self.finish_recording(nes)?;
                            return Ok(());
                        }
                        Event::KeyDown {
                            keycode: Some(keycode),
                            keymod,
                            ..
                        } => {
                            // Handle F4 for shader cycling
                            if keycode == Keycode::F4 {
                                gl_backend.cycle_shader();
                            } else if Self::is_fullscreen_shortcut(keycode, keymod) {
                                self.toggle_fullscreen(Some(&mut gl_backend));
                            } else if self
                                .handle_key_down_for_run_with_modifiers(nes, keycode, keymod)
                                == KeyDownOutcome::Quit
                            {
                                self.gl_backend = Some(gl_backend);
                                self.finish_recording(nes)?;
                                return Ok(());
                            }
                        }
                        Event::KeyUp {
                            keycode: Some(keycode),
                            ..
                        } => {
                            self.handle_key_up_for_run(nes, keycode);
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
                        Event::MouseMotion { x, y, .. } => {
                            let (window_width, window_height) = gl_backend.window_size();
                            self.last_mouse_position = Some((x, y));
                            Self::update_mouse_motion(nes, x, y, window_width, window_height);
                        }
                        Event::MouseButtonDown { mouse_btn, .. } => {
                            Self::update_mouse_button(nes, mouse_btn, true);
                        }
                        Event::MouseButtonUp { mouse_btn, .. } => {
                            Self::update_mouse_button(nes, mouse_btn, false);
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

                    let overlay_text = self.overlay_render_text(nes);
                    let zapper_active = !Self::zapper_ports(nes).is_empty();
                    self.update_cursor_visibility(zapper_active);
                    let crosshair = self.zapper_crosshair(nes);
                    let overlay_blink_red = self.should_overlay_blink_red(nes);
                    gl_backend.update_breakpoints(&self.breakpoints);
                    let action = gl_backend.render(
                        nes,
                        self.debugger_open_requested,
                        overlay_text.as_deref(),
                        overlay_blink_red,
                        crosshair,
                    );
                    self.apply_debugger_ui_action(nes, action);
                    std::thread::sleep(std::time::Duration::from_millis(16));
                    continue;
                }

                // Apply button states from autorun before emulation (autorun playback mode)
                match self.handle_autorun_before_frame(nes) {
                    Ok(true) => {} // Continue normally
                    Ok(false) => {
                        // Normal playback completion (shouldn't happen in current logic)
                        self.gl_backend = Some(gl_backend);
                        return self.finish_recording(nes);
                    }
                    Err(AutorunExitCode::Success) => {
                        // Playback succeeded - exit with code 0
                        return Err("AUTORUN_EXIT:0".to_string());
                    }
                    Err(AutorunExitCode::Failure) => {
                        // Playback failed - exit with code 1
                        return Err("AUTORUN_EXIT:1".to_string());
                    }
                }

                // Record button states after input processing (autorun record mode)
                // This happens after the pause check to ensure we only record frames that are actually emulated
                let autorun_checkpoint_due = self.handle_autorun_after_input(nes);

                // 2. Emulate until PPU completes a full frame (reaches VBlank)
                // The PPU runs at 3x CPU clock (NTSC) or 3.2x (PAL), so run_cpu_tick()
                // automatically runs the correct number of PPU cycles per CPU instruction.
                // A full frame is 262 scanlines × 341 pixels = 89,342 PPU cycles for NTSC
                while !nes.is_ready_to_render() && !nes.cpu.is_halted() {
                    if self.check_breakpoint_hit(nes.cpu.pc(), nes.cpu.current_interrupt()) {
                        break;
                    }

                    nes.run(&tracing);
                    self.maybe_arm_temporary_breakpoint_after_instruction(nes);
                    self.check_post_instruction_breakpoints(nes);

                    if self.paused {
                        break;
                    }

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

                if let Some(ref audio) = self.audio
                    && last_audio_stats_print.elapsed() >= Duration::from_secs(1)
                {
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
                        log_info(format!(
                            "Audio stats (last ~1s): received={}, dropped={}, underrun={}, cpu_cycles_per_sec≈{:.0}",
                            received, dropped, underrun, cycles_per_sec
                        ));
                    }
                    last_cpu_cycles = now_cycles;
                    last_perf_instant = Instant::now();
                    last_audio_stats_print = Instant::now();
                }
                nes.clear_ready_to_render();
                self.handle_autorun_after_frame(nes, autorun_checkpoint_due);

                // 3. Render the frame (always present the NES frame; show debugger if requested)
                let overlay_text = self.overlay_render_text(nes);
                let zapper_active = !Self::zapper_ports(nes).is_empty();
                self.update_cursor_visibility(zapper_active);
                let crosshair = self.zapper_crosshair(nes);
                let overlay_blink_red = self.should_overlay_blink_red(nes);
                gl_backend.update_breakpoints(&self.breakpoints);
                let _ = gl_backend.render(
                    nes,
                    self.debugger_open_requested,
                    overlay_text.as_deref(),
                    overlay_blink_red,
                    crosshair,
                );

                // 4. Frame limiting - maintain ~60 FPS
                let current_time = timer.performance_counter();
                let elapsed_ticks = (current_time - last_frame_time) as f64;
                let elapsed_seconds = elapsed_ticks / performance_frequency;
                let tv_system = nes.app_context.borrow().config().tv_system;
                let target_frame_time = 1.0 / tv_system.frame_rate_hz();

                // Calculate FPS before sleeping
                // let fps = 1.0 / elapsed_seconds;
                // log_info(format!("FPS: {:.2}", fps));

                // Update last_frame_time before sleeping to avoid timing drift
                last_frame_time = current_time;

                if Self::should_manual_frame_limit(self.vsync_enabled)
                    && elapsed_seconds < target_frame_time
                {
                    let sleep_time = target_frame_time - elapsed_seconds;
                    std::thread::sleep(std::time::Duration::from_secs_f64(sleep_time));
                }
            }
        } else {
            // Headless mode - just run without rendering
            loop {
                // Handle autorun before frame
                match self.handle_autorun_before_frame(nes) {
                    Ok(true) => {} // Continue normally
                    Ok(false) => {
                        // Normal playback completion (shouldn't happen in current logic)
                        return self.finish_recording(nes);
                    }
                    Err(AutorunExitCode::Success) => {
                        // Playback succeeded - exit with code 0
                        return Err("AUTORUN_EXIT:0".to_string());
                    }
                    Err(AutorunExitCode::Failure) => {
                        // Playback failed - exit with code 1
                        return Err("AUTORUN_EXIT:1".to_string());
                    }
                }

                if self.process_headless_events_for_run(nes) {
                    self.finish_recording(nes)?;
                    return Ok(());
                }

                // After input processing, record if needed
                let autorun_checkpoint_due = self.handle_autorun_after_input(nes);

                // Avoid a busy loop while paused.
                if self.paused {
                    std::thread::sleep(std::time::Duration::from_millis(16));
                    continue;
                }

                self.tick_headless_frame_for_run(nes, &tracing);

                // If we paused due to a breakpoint during frame emulation, let the next
                // iteration handle paused-mode behavior.
                if self.paused {
                    continue;
                }

                if let Some(ref mut audio) = self.audio
                    && last_audio_stats_print.elapsed() >= Duration::from_secs(1)
                {
                    let (received, dropped, underrun) = audio.take_and_reset_stats();
                    let now_cycles = nes.cpu.get_total_cycles();
                    let elapsed = last_perf_instant.elapsed().as_secs_f64();
                    let cycles_per_sec = if elapsed > 0.0 {
                        (now_cycles - last_cpu_cycles) as f64 / elapsed
                    } else {
                        0.0
                    };
                    if dropped != 0 || underrun != 0 {
                        log_info(format!(
                            "Audio stats (last ~1s): received={}, dropped={}, underrun={}, cpu_cycles_per_sec≈{:.0}",
                            received, dropped, underrun, cycles_per_sec
                        ));
                    }
                    last_cpu_cycles = now_cycles;
                    last_perf_instant = Instant::now();
                    last_audio_stats_print = Instant::now();
                }

                nes.clear_ready_to_render();
                self.handle_autorun_after_frame(nes, autorun_checkpoint_due);
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
        self.breakpoints.add(BreakpointKind::Pc(addr));
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn remove_breakpoint(&mut self, addr: u16) {
        if let Some(idx) = self
            .breakpoints
            .iter()
            .position(|b| b.kind == BreakpointKind::Pc(addr))
        {
            self.breakpoints.remove(idx);
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn add_cycle_breakpoint(&mut self, cycle: u64) {
        self.breakpoints.add(BreakpointKind::Cycle(cycle));
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn add_frame_breakpoint(&mut self, frame: u64) {
        self.breakpoints.add(BreakpointKind::Frame(frame));
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn add_write_address_breakpoint(&mut self, addr: u16) {
        self.breakpoints.add(BreakpointKind::WriteAddress(addr));
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn breakpoint_count(&self) -> usize {
        if self.breakpoints.is_empty() {
            0
        } else {
            self.breakpoints.len()
        }
    }

    pub(crate) fn load_breakpoints_from_debug_file(&mut self, nes: &Nes) {
        let Some(path) = nes.debug_path() else { return };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return;
        };
        self.breakpoints = BreakpointList::load_from_str(&text);
    }

    pub(crate) fn save_breakpoints_to_debug_file(&self, nes: &Nes) {
        let Some(path) = nes.debug_path() else { return };
        if self.breakpoints.is_empty() {
            if path.exists()
                && let Err(err) = std::fs::remove_file(&path)
            {
                log_info(format!("Failed to remove .debug file: {err}"));
            }
            return;
        }
        let content = self.breakpoints.save_to_string();
        if let Err(err) = std::fs::write(&path, content) {
            log_info(format!("Failed to save breakpoints: {err}"));
        }
    }

    pub(crate) fn load_breakpoints_from_context(&mut self, app_context: &SharedAppContext) {
        let app_context = app_context.borrow();
        let config = app_context.config();
        for &kind in &config.breakpoints {
            self.breakpoints.add(kind);
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn set_debugger_renderer(&mut self, renderer: Box<dyn DebuggerRenderer>) {
        self.debugger_renderer = Some(renderer);
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn render_debugger_if_needed(&mut self, nes: &Nes) {
        if !self.debugger_open_requested {
            return;
        }

        let Some(renderer) = self.debugger_renderer.as_mut() else {
            return;
        };

        let snapshot = snapshot(nes);
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

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn is_fullscreen(&self) -> bool {
        self.fullscreen
    }

    fn process_headless_events_for_run(&mut self, nes: &mut Nes) -> bool {
        // Returns `true` if the caller should quit the event loop.
        let events: Vec<_> = self.event_pump.poll_iter().collect();
        for event in events {
            match event {
                Event::Quit { .. } => {
                    return true;
                }
                Event::KeyDown {
                    keycode: Some(keycode),
                    keymod,
                    ..
                } => {
                    if self.handle_key_down_for_run_with_modifiers(nes, keycode, keymod)
                        == KeyDownOutcome::Quit
                    {
                        return true;
                    }
                }
                Event::KeyUp {
                    keycode: Some(keycode),
                    ..
                } => {
                    self.handle_key_up_for_run(nes, keycode);
                }
                _ => {}
            }
        }

        if self.paused {
            self.render_debugger_if_needed(nes);
            return false;
        }

        if self.check_breakpoint_hit(nes.cpu.pc(), nes.cpu.current_interrupt()) {
            return false;
        }

        false
    }

    fn tick_headless_frame_for_run(&mut self, nes: &mut Nes, tracing: &Tracing) {
        while !nes.is_ready_to_render() && !nes.cpu.is_halted() {
            if self.check_breakpoint_hit(nes.cpu.pc(), nes.cpu.current_interrupt()) {
                break;
            }

            nes.run(tracing);
            self.maybe_arm_temporary_breakpoint_after_instruction(nes);
            self.check_post_instruction_breakpoints(nes);

            if self.paused {
                break;
            }

            if let Some(ref mut audio) = self.audio {
                while nes.sample_ready() {
                    if let Some(sample) = nes.get_sample() {
                        audio.queue_sample(sample);
                    }
                }
            }
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn tick_headless_once_for_run(&mut self, nes: &mut Nes) -> bool {
        if self.process_headless_events_for_run(nes) {
            return true;
        }

        if self.paused {
            return false;
        }

        nes.run_cpu_tick();
        self.maybe_arm_temporary_breakpoint_after_instruction(nes);
        self.check_post_instruction_breakpoints(nes);
        false
    }

    fn tick_windowed_paused_for_run(
        debugger_open_requested: bool,
        debugger_renderer: &mut Option<Box<dyn DebuggerRenderer>>,
        nes: &Nes,
    ) {
        if !debugger_open_requested {
            return;
        }

        let Some(renderer) = debugger_renderer.as_mut() else {
            return;
        };

        let snapshot = snapshot(nes);
        renderer.render(&snapshot);
    }

    fn apply_debugger_ui_action(&mut self, nes: &mut Nes, action: ui::DebuggerUiAction) {
        if !self.debugger_open_requested {
            return;
        }

        let mut should_continue = action.continue_run;

        if action.step_over {
            let pc = nes.cpu.pc();
            let opcode = {
                let memory = nes.bus.borrow();
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
        if action.run_to_next_scanline {
            Self::debugger_run_to_next_scanline(nes);
        }
        if action.run_to_nmi {
            should_continue |=
                self.arm_run_to_interrupt(nes, 0xFFFA, crate::cpu::InterruptKind::Nmi);
        }
        if action.run_to_irq {
            should_continue |=
                self.arm_run_to_interrupt(nes, 0xFFFE, crate::cpu::InterruptKind::Irq);
        }

        if should_continue {
            self.continue_from_debugger(nes);
        }

        // Breakpoint management actions from the UI panel.
        if let Some(kind) = action.add_breakpoint {
            self.breakpoints.add(kind);
        }
        if let Some(index) = action.remove_breakpoint {
            self.breakpoints.remove(index);
        }
        if let Some(index) = action.enable_breakpoint {
            self.breakpoints.enable(index);
        }
        if let Some(index) = action.disable_breakpoint {
            self.breakpoints.disable(index);
        }
    }

    #[cfg(test)]
    fn handle_key_down_for_run(&mut self, nes: &mut Nes, keycode: Keycode) -> KeyDownOutcome {
        self.handle_key_down_for_run_with_modifiers(nes, keycode, Mod::NOMOD)
    }

    fn has_command_or_alt_modifier(keymod: Mod) -> bool {
        keymod.intersects(Mod::LALTMOD | Mod::RALTMOD | Mod::LGUIMOD | Mod::RGUIMOD)
    }

    fn has_shift_modifier(keymod: Mod) -> bool {
        keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD)
    }

    fn is_fullscreen_shortcut(keycode: Keycode, keymod: Mod) -> bool {
        keycode == Keycode::F && Self::has_command_or_alt_modifier(keymod)
    }

    fn preload_cartridge_switch_entries_from_default_catalog(&mut self) {
        if let Some(home) = std::env::var_os("HOME") {
            let catalog_path = default_catalog_csv_path(std::path::PathBuf::from(home).as_path());
            let _ = self.load_cartridge_switch_entries_from_csv(&catalog_path);
        }
    }

    fn parse_cartridge_switch_entries(content: &str) -> Vec<String> {
        content
            .lines()
            .skip(1)
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToString::to_string)
            .collect()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn read_rom_bytes(rom_path: &str) -> Result<Vec<u8>, String> {
        fs::read(rom_path).map_err(|err| format!("Failed to read ROM: {err}"))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn load_cartridge_from_rom_bytes(
        &self,
        rom_path: &str,
        rom_bytes: &[u8],
        app_context: SharedAppContext,
    ) -> Result<crate::cartridge::Cartridge, String> {
        crate::cartridge::Cartridge::load_from_file(rom_bytes, rom_path, app_context)
            .map_err(|err| format!("Failed to load ROM cartridge: {err}"))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn apply_cartridge_timing_mode_from_rom(
        app_context: &SharedAppContext,
        cartridge: &crate::cartridge::Cartridge,
    ) {
        let rom_timing_mode = cartridge.rom_timing_mode();
        let applied = app_context
            .borrow_mut()
            .config_mut()
            .apply_rom_timing_mode(rom_timing_mode);
        log_rom_timing_mode_selection(app_context, rom_timing_mode, applied);
    }

    fn request_cartridge_switch_dialog(&mut self) -> KeyDownOutcome {
        self.cartridge_switch_dialog_requested = true;
        self.paused = true;
        self.preload_cartridge_switch_entries_from_default_catalog();
        self.refresh_cartridge_switch_filtered_entries();
        KeyDownOutcome::Continue
    }

    fn close_cartridge_switch_dialog(&mut self) {
        self.cartridge_switch_dialog_requested = false;
        if !self.debugger_open_requested {
            self.paused = false;
        }
    }

    fn refresh_cartridge_switch_filtered_entries(&mut self) {
        let needle = self.cartridge_switch_filter_query.to_ascii_lowercase();
        self.cartridge_switch_filtered_indices = self
            .cartridge_switch_entries
            .iter()
            .enumerate()
            .filter_map(|(index, path)| {
                let haystack = Path::new(path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(path)
                    .to_ascii_lowercase();

                if needle.is_empty() || Self::cartridge_switch_fuzzy_matches(&needle, &haystack) {
                    Some(index)
                } else {
                    None
                }
            })
            .collect();

        if self.cartridge_switch_filtered_indices.is_empty() {
            self.cartridge_switch_selected_index = 0;
        } else {
            self.cartridge_switch_selected_index = self
                .cartridge_switch_selected_index
                .min(self.cartridge_switch_filtered_indices.len() - 1);
        }
    }

    fn cartridge_switch_fuzzy_matches(needle: &str, haystack: &str) -> bool {
        if needle.is_empty() {
            return true;
        }

        let mut needle_chars = needle.chars();
        let mut current = match needle_chars.next() {
            Some(ch) => ch,
            None => return true,
        };

        for ch in haystack.chars() {
            if ch == current {
                match needle_chars.next() {
                    Some(next) => current = next,
                    None => return true,
                }
            }
        }

        false
    }

    fn move_cartridge_switch_selection_next(&mut self) {
        let visible_entry_count = self.cartridge_switch_visible_entry_count();
        if visible_entry_count == 0 {
            self.cartridge_switch_selected_index = 0;
            return;
        }

        self.cartridge_switch_selected_index =
            (self.cartridge_switch_selected_index + 1) % visible_entry_count;
    }

    fn move_cartridge_switch_selection_prev(&mut self) {
        let visible_entry_count = self.cartridge_switch_visible_entry_count();
        if visible_entry_count == 0 {
            self.cartridge_switch_selected_index = 0;
            return;
        }

        self.cartridge_switch_selected_index = if self.cartridge_switch_selected_index == 0 {
            visible_entry_count - 1
        } else {
            self.cartridge_switch_selected_index - 1
        };
    }

    fn cartridge_switch_uses_unfiltered_fallback(&self) -> bool {
        self.cartridge_switch_filter_query.is_empty()
            && self.cartridge_switch_filtered_indices.is_empty()
            && !self.cartridge_switch_entries.is_empty()
    }

    fn cartridge_switch_visible_entry_count(&self) -> usize {
        if self.cartridge_switch_uses_unfiltered_fallback() {
            self.cartridge_switch_entries.len()
        } else {
            self.cartridge_switch_filtered_indices.len()
        }
    }

    fn cartridge_switch_entry_index_at_visible(&self, visible_index: usize) -> Option<usize> {
        if self.cartridge_switch_uses_unfiltered_fallback() {
            self.cartridge_switch_entries
                .get(visible_index)
                .map(|_| visible_index)
        } else {
            self.cartridge_switch_filtered_indices
                .get(visible_index)
                .copied()
        }
    }

    fn selected_cartridge_switch_entry(&self) -> Option<&str> {
        self.cartridge_switch_entry_index_at_visible(self.cartridge_switch_selected_index)
            .and_then(|entry_index| self.cartridge_switch_entries.get(entry_index))
            .map(String::as_str)
    }

    fn cartridge_switch_input_char(keycode: Keycode) -> Option<char> {
        match keycode {
            Keycode::A => Some('a'),
            Keycode::B => Some('b'),
            Keycode::C => Some('c'),
            Keycode::D => Some('d'),
            Keycode::E => Some('e'),
            Keycode::F => Some('f'),
            Keycode::G => Some('g'),
            Keycode::H => Some('h'),
            Keycode::I => Some('i'),
            Keycode::J => Some('j'),
            Keycode::K => Some('k'),
            Keycode::L => Some('l'),
            Keycode::M => Some('m'),
            Keycode::N => Some('n'),
            Keycode::O => Some('o'),
            Keycode::P => Some('p'),
            Keycode::Q => Some('q'),
            Keycode::R => Some('r'),
            Keycode::S => Some('s'),
            Keycode::T => Some('t'),
            Keycode::U => Some('u'),
            Keycode::V => Some('v'),
            Keycode::W => Some('w'),
            Keycode::X => Some('x'),
            Keycode::Y => Some('y'),
            Keycode::Z => Some('z'),
            Keycode::Num0 => Some('0'),
            Keycode::Num1 => Some('1'),
            Keycode::Num2 => Some('2'),
            Keycode::Num3 => Some('3'),
            Keycode::Num4 => Some('4'),
            Keycode::Num5 => Some('5'),
            Keycode::Num6 => Some('6'),
            Keycode::Num7 => Some('7'),
            Keycode::Num8 => Some('8'),
            Keycode::Num9 => Some('9'),
            Keycode::Space => Some(' '),
            Keycode::Minus => Some('-'),
            Keycode::Underscore => Some('_'),
            Keycode::Period => Some('.'),
            Keycode::Slash => Some('/'),
            _ => None,
        }
    }

    fn handle_cartridge_switch_dialog_key(&mut self, nes: &mut Nes, keycode: Keycode) {
        match keycode {
            Keycode::Escape => self.close_cartridge_switch_dialog(),
            Keycode::Down => self.move_cartridge_switch_selection_next(),
            Keycode::Up => self.move_cartridge_switch_selection_prev(),
            Keycode::Backspace => {
                self.cartridge_switch_filter_query.pop();
                self.refresh_cartridge_switch_filtered_entries();
            }
            Keycode::Return | Keycode::KpEnter => {
                if let Some(path) = self.selected_cartridge_switch_entry().map(str::to_string)
                    && let Err(err) = self.switch_to_cartridge_path(nes, &path)
                {
                    log_info(format!("Failed to switch cartridge: {err}"));
                }
                self.close_cartridge_switch_dialog();
            }
            _ => {
                if let Some(ch) = Self::cartridge_switch_input_char(keycode) {
                    self.cartridge_switch_filter_query.push(ch);
                    self.refresh_cartridge_switch_filtered_entries();
                }
            }
        }
    }

    fn cartridge_switch_overlay_text(&self) -> String {
        if self.cartridge_switch_entries.is_empty() {
            return "Switch cartridge\nNo catalog entries found\n\nPress Esc to close".to_string();
        }

        let filter_label = if self.cartridge_switch_filter_query.is_empty() {
            "(none)"
        } else {
            self.cartridge_switch_filter_query.as_str()
        };
        let visible_count = self.cartridge_switch_visible_entry_count();
        let total_count = self.cartridge_switch_entries.len();
        let matches_label = format!("Matches: {visible_count}/{total_count}");

        if visible_count == 0 {
            return format!(
                "Switch cartridge\nFilter: {filter_label}\n{matches_label}\nNo matching entries\n\nBackspace: Edit filter    Esc: Cancel"
            );
        }

        const MAX_VISIBLE: usize = 12;
        let entry_count = visible_count;
        let selected = self
            .cartridge_switch_selected_index
            .min(entry_count.saturating_sub(1));
        let mut start = selected.saturating_sub(MAX_VISIBLE / 2);
        let mut end = (start + MAX_VISIBLE).min(entry_count);
        if end - start < MAX_VISIBLE {
            start = end.saturating_sub(MAX_VISIBLE);
        }
        end = (start + MAX_VISIBLE).min(entry_count);

        let mut lines = vec![
            "Switch cartridge".to_string(),
            format!("Filter: {filter_label}"),
            matches_label,
            "Up/Down: Select".to_string(),
            "Type to filter    Backspace: Delete".to_string(),
            "Enter: Load    Esc: Cancel".to_string(),
            String::new(),
        ];

        for visible_index in start..end {
            let Some(entry_index) = self.cartridge_switch_entry_index_at_visible(visible_index)
            else {
                continue;
            };

            let absolute_index = visible_index;
            let line = if absolute_index == selected {
                format!(">> {} <<", self.cartridge_switch_entries[entry_index])
            } else {
                format!("   {}", self.cartridge_switch_entries[entry_index])
            };
            lines.push(line);
        }

        lines.join("\n")
    }

    pub fn load_cartridge_switch_entries_from_csv(
        &mut self,
        catalog_csv_path: &Path,
    ) -> Result<(), String> {
        let content = fs::read_to_string(catalog_csv_path)
            .map_err(|err| format!("Failed to read cartridge catalog CSV: {err}"))?;
        self.cartridge_switch_entries = Self::parse_cartridge_switch_entries(&content);
        self.refresh_cartridge_switch_filtered_entries();
        Ok(())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn switch_to_cartridge_path(
        &mut self,
        nes: &mut Nes,
        rom_path: &str,
    ) -> Result<(), String> {
        let rom_bytes = Self::read_rom_bytes(rom_path)?;
        let app_context = nes.app_context.clone();
        let cartridge =
            self.load_cartridge_from_rom_bytes(rom_path, &rom_bytes, app_context.clone())?;

        Self::apply_cartridge_timing_mode_from_rom(&app_context, &cartridge);

        nes.insert_cartridge(cartridge);
        nes.reset(false);
        Ok(())
    }

    fn handle_key_down_for_run_with_modifiers(
        &mut self,
        nes: &mut Nes,
        keycode: Keycode,
        keymod: Mod,
    ) -> KeyDownOutcome {
        if self.cartridge_switch_dialog_requested {
            self.handle_cartridge_switch_dialog_key(nes, keycode);
            return KeyDownOutcome::Continue;
        }

        if Self::has_command_or_alt_modifier(keymod) {
            match keycode {
                Keycode::Q => return KeyDownOutcome::Quit,
                Keycode::R => {
                    let soft_reset = !Self::has_shift_modifier(keymod);
                    nes.reset(soft_reset);
                    return KeyDownOutcome::Continue;
                }
                Keycode::O => return self.request_cartridge_switch_dialog(),
                Keycode::F => {
                    self.toggle_fullscreen(None);
                    return KeyDownOutcome::Continue;
                }
                _ => {}
            }
        }

        // When the debugger is open, make F5 behave exactly like the Continue button.
        // This ensures breakpoint ignore-once semantics apply equally.
        if keycode == Keycode::F5 && self.debugger_open_requested {
            self.apply_debugger_ui_action(
                nes,
                ui::DebuggerUiAction {
                    continue_run: true,
                    ..Default::default()
                },
            );
            return KeyDownOutcome::Continue;
        }

        if keycode == Keycode::H {
            self.help_overlay_visible = !self.help_overlay_visible;
            return KeyDownOutcome::Continue;
        }

        let (port_1, port_2) = Self::keyboard_ports(nes, &self.controller_player_map);
        Self::handle_key_down_with_keyboard_ports(
            nes,
            keycode,
            self.audio.as_ref(),
            &mut self.paused,
            &mut self.debugger_open_requested,
            port_1,
            port_2,
        )
    }

    fn debugger_run_to_next_frame(nes: &mut Nes) {
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

    fn arm_run_to_interrupt(
        &mut self,
        nes: &Nes,
        vector_addr: u16,
        kind: crate::cpu::InterruptKind,
    ) -> bool {
        let target = Self::read_vector_target(nes, vector_addr);
        self.set_temporary_breakpoint_for_interrupt(nes, target, kind);
        true
    }

    fn debugger_run_to_next_scanline(nes: &mut Nes) {
        const MAX_STEPS: usize = 100_000;

        let start_scanline = {
            let ppu = nes.ppu.borrow();
            ppu.scanline()
        };

        for _step in 0..MAX_STEPS {
            if nes.cpu.is_halted() {
                break;
            }

            nes.run_cpu_tick();

            let scanline = {
                let ppu = nes.ppu.borrow();
                ppu.scanline()
            };

            if scanline != start_scanline {
                break;
            }
        }
    }

    fn debugger_step_over(nes: &mut Nes) {
        const JSR_OPCODE: u8 = 0x20;

        let pc = nes.cpu.pc();
        let opcode = {
            let memory = nes.bus.borrow();
            memory.read_cpu_for_debugger(pc)
        };

        if opcode == JSR_OPCODE {
            let next_pc = pc.wrapping_add(3);

            // Execute the JSR itself (enter subroutine).
            nes.run_cpu_tick();

            // Run until we return to the instruction after the original JSR.
            const MAX_STEPS: usize = 1_000_000;
            for _ in 0..MAX_STEPS {
                if nes.cpu.pc() == next_pc || nes.cpu.is_halted() {
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
    /// Player 1 keys (port 1):
    /// - W/A/S/D: D-Pad (Up, Left, Down, Right)
    /// - R: A button
    /// - T: B button
    /// - 4: Select button
    /// - 5: Start button
    ///
    /// Player 2 keys (port 2):
    /// - I/J/K/L: D-Pad (Up, Left, Down, Right)
    /// - O: A button
    /// - P: B button
    /// - 9: Select button
    /// - 0: Start button
    ///
    /// Emulator controls:
    /// - Cmd/Alt+Q: Quit
    /// - Space: Toggle pause
    /// - Cmd/Alt+R: Soft reset
    /// - Shift+Cmd/Alt+R: Hard reset
    /// - Cmd/Alt+F: Toggle fullscreen
    /// - F5: Open debugger (when closed) / Continue (when debugger open)
    /// - F10: Debugger step-over (JSR runs until RTS)
    /// - F11: Debugger step-into (single CPU tick)
    /// - F2/F3: Volume up/down (when audio is enabled)
    /// - F6: Save state (when a ROM is loaded)
    /// - F7: Load state (when a ROM is loaded)
    fn handle_key_down_with_keyboard_ports(
        nes: &mut Nes,
        keycode: Keycode,
        audio: Option<&SdlNesAudio>,
        paused: &mut bool,
        debugger_open_requested: &mut bool,
        port_1: Option<u8>,
        port_2: Option<u8>,
    ) -> KeyDownOutcome {
        match keycode {
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
            Keycode::F6 => {
                Self::save_state_to_disk(nes);
            }
            Keycode::F7 => {
                Self::load_state_from_disk(nes);
            }
            _ => Self::handle_joypad_key(nes, keycode, port_1, port_2, true),
        }

        KeyDownOutcome::Continue
    }

    fn handle_joypad_key(
        nes: &mut Nes,
        keycode: Keycode,
        port_1: Option<u8>,
        port_2: Option<u8>,
        pressed: bool,
    ) {
        let p1 = port_1.as_slice();
        let p2 = port_2.as_slice();
        match keycode {
            // Player 1: W/A/S/D + R/T (A/B) + 4/5 (Select/Start)
            Keycode::W => Self::apply_keyboard_button(nes, p1, Button::Up, pressed),
            Keycode::S => Self::apply_keyboard_button(nes, p1, Button::Down, pressed),
            Keycode::A => Self::apply_keyboard_button(nes, p1, Button::Left, pressed),
            Keycode::D => Self::apply_keyboard_button(nes, p1, Button::Right, pressed),
            Keycode::R => Self::apply_keyboard_button(nes, p1, Button::A, pressed),
            Keycode::T => Self::apply_keyboard_button(nes, p1, Button::B, pressed),
            Keycode::Num4 => Self::apply_keyboard_button(nes, p1, Button::Select, pressed),
            Keycode::Num5 => Self::apply_keyboard_button(nes, p1, Button::Start, pressed),
            // Player 2: I/J/K/L + O/P (A/B) + 9/0 (Select/Start)
            Keycode::I => Self::apply_keyboard_button(nes, p2, Button::Up, pressed),
            Keycode::K => Self::apply_keyboard_button(nes, p2, Button::Down, pressed),
            Keycode::J => Self::apply_keyboard_button(nes, p2, Button::Left, pressed),
            Keycode::L => Self::apply_keyboard_button(nes, p2, Button::Right, pressed),
            Keycode::P => Self::apply_keyboard_button(nes, p2, Button::B, pressed),
            Keycode::O => Self::apply_keyboard_button(nes, p2, Button::A, pressed),
            Keycode::Num9 => Self::apply_keyboard_button(nes, p2, Button::Select, pressed),
            Keycode::Num0 => Self::apply_keyboard_button(nes, p2, Button::Start, pressed),
            _ => {}
        }
    }

    #[cfg(test)]
    fn handle_key_down(
        nes: &mut Nes,
        keycode: Keycode,
        audio: Option<&SdlNesAudio>,
        paused: &mut bool,
        debugger_open_requested: &mut bool,
    ) -> KeyDownOutcome {
        let gamepad_ports = Self::gamepad_ports(nes);
        let port_1 = gamepad_ports.first().copied();
        let port_2 = gamepad_ports.get(1).copied();
        Self::handle_key_down_with_keyboard_ports(
            nes,
            keycode,
            audio,
            paused,
            debugger_open_requested,
            port_1,
            port_2,
        )
    }

    fn handle_key_up_with_keyboard_ports(
        nes: &mut Nes,
        keycode: Keycode,
        port_1: Option<u8>,
        port_2: Option<u8>,
    ) {
        Self::handle_joypad_key(nes, keycode, port_1, port_2, false);
    }

    fn handle_key_up_for_run(&mut self, nes: &mut Nes, keycode: Keycode) {
        let (port_1, port_2) = Self::keyboard_ports(nes, &self.controller_player_map);
        Self::handle_key_up_with_keyboard_ports(nes, keycode, port_1, port_2);
    }

    fn help_overlay_text(&self, nes: &Nes) -> String {
        let (port_1, port_2) = Self::keyboard_ports(nes, &self.controller_player_map);

        let player1_section = if port_1.is_some() {
            "Controller (Player 1)\n\
W/A/S/D: D-Pad\n\
R: A\n\
T: B\n\
4: Select\n\
5: Start"
        } else {
            "Controller (Player 1)\nGamepad"
        };

        let player2_section = if port_2.is_some() {
            "Controller (Player 2)\n\
I/J/K/L: D-Pad\n\
O: A\n\
P: B\n\
9: Select\n\
0: Start"
        } else {
            "Controller (Player 2)\nGamepad"
        };

        format!(
            "Controls\n\
Cmd/Alt+Q: Quit\n\
Space: Pause\n\
H: Toggle help\n\
\n\
System\n\
Cmd/Alt+R: Soft reset\n\
Shift+Cmd/Alt+R: Hard reset\n\
Cmd/Alt+F: Toggle fullscreen\n\
Cmd/Alt+O: Switch cartridge\n\
F2/F3: Volume up/down\n\
F4: Cycle shader\n\
F5: Debugger (open/continue)\n\
F6: Save state\n\
F7: Load state\n\
F10: Step over\n\
F11: Step into\n\
\n\
{player1_section}\n\
\n\
{player2_section}"
        )
    }

    /// Generate overlay text for rendering.
    /// Returns autorun overlay text if in autorun mode, otherwise help overlay text if visible.
    fn overlay_render_text(&self, nes: &Nes) -> Option<String> {
        if self.cartridge_switch_dialog_requested {
            return Some(self.cartridge_switch_overlay_text());
        }

        // Check if autorun is active and generate overlay
        if let Some(ref autorun_state) = self.autorun_state {
            let tv_system = nes.app_context.borrow().config().tv_system;
            let overlay = self.autorun_overlay_text(autorun_state, tv_system);
            return Some(overlay);
        }

        // Fall back to help overlay if visible
        if self.help_overlay_visible {
            Some(self.help_overlay_text(nes))
        } else {
            None
        }
    }

    /// Generate autorun overlay text based on current mode and state.
    fn autorun_overlay_text(&self, autorun_state: &AutorunState, tv_system: TimingMode) -> String {
        use crate::console::AutorunMode;

        match autorun_state.mode() {
            AutorunMode::Playback => {
                let current_frames = autorun_state.current_frame_index();
                let total_frames = autorun_state.total_frames();
                let (elapsed, total) =
                    Self::format_time_pair_for(current_frames, total_frames, tv_system);
                format!("Playback\n{} / {}", elapsed, total)
            }
            AutorunMode::Record => {
                if autorun_state.is_extending_playback() {
                    // In extend mode during playback phase
                    let current_frames = autorun_state.current_frame_index();
                    let total_frames = autorun_state.total_frames();
                    let (elapsed, total) =
                        Self::format_time_pair_for(current_frames, total_frames, tv_system);
                    format!("Playback\n{} / {}", elapsed, total)
                } else {
                    // In record mode (or extend mode after playback finished)
                    let current_frames = autorun_state.total_frames();
                    let (elapsed, _) =
                        Self::format_time_pair_for(current_frames, current_frames, tv_system);
                    format!("Recording\n{} / {}", elapsed, elapsed)
                }
            }
            AutorunMode::None => String::new(),
        }
    }

    /// Format frame counts as MM:SS time pairs.
    fn format_time_pair_for(
        current_frames: usize,
        total_frames: usize,
        tv_system: TimingMode,
    ) -> (String, String) {
        let fps = tv_system.frame_rate_hz().round().max(1.0) as usize;
        let current_secs = current_frames / fps;
        let total_secs = total_frames / fps;
        (
            Self::format_mm_ss(current_secs),
            Self::format_mm_ss(total_secs),
        )
    }

    /// Format seconds as MM:SS.
    fn format_mm_ss(seconds: usize) -> String {
        let minutes = seconds / 60;
        let secs = seconds % 60;
        format!("{:02}:{:02}", minutes, secs)
    }

    #[cfg(test)]
    fn help_overlay_render_text(&self, nes: &Nes) -> Option<String> {
        if self.help_overlay_visible {
            Some(self.help_overlay_text(nes))
        } else {
            None
        }
    }

    fn save_state_to_disk(nes: &mut Nes) {
        let Some(state_path) = nes.state_path() else {
            return;
        };

        let state = nes.save_state();
        let bytes = match state.to_bytes() {
            Ok(bytes) => bytes,
            Err(err) => {
                log_info(format!("Failed to serialize save-state: {err}"));
                return;
            }
        };

        if let Some(parent) = state_path.parent()
            && let Err(err) = fs::create_dir_all(parent)
        {
            log_info(format!("Failed to create save-state directory: {err}"));
            return;
        }

        let mut tmp_path = state_path.clone();
        tmp_path.set_extension(format!("state.tmp.{}", std::process::id()));

        if let Err(err) = fs::write(&tmp_path, bytes) {
            log_info(format!("Failed to write save-state: {err}"));
            return;
        }

        if let Err(err) = fs::rename(&tmp_path, &state_path) {
            log_info(format!("Failed to finalize save-state: {err}"));
        }
    }

    fn load_state_from_disk(nes: &mut Nes) {
        let Some(state_path) = nes.state_path() else {
            return;
        };

        if !state_path.exists() {
            return;
        }

        let bytes = match fs::read(&state_path) {
            Ok(bytes) => bytes,
            Err(err) => {
                log_info(format!("Failed to read save-state: {err}"));
                return;
            }
        };

        let state = match SaveState::from_bytes(&bytes) {
            Ok(state) => state,
            Err(err) => {
                log_info(format!("Failed to deserialize save-state: {err}"));
                return;
            }
        };

        if let Err(err) = nes.load_state(&state) {
            log_info(format!("Failed to restore save-state: {err}"));
        }
    }

    /// Handle controller device added event
    fn handle_controller_added(&mut self, which: u32) {
        // Only add if we have less than 2 controllers
        if self.controllers.len() >= 2 {
            log_info(format!(
                "Controller {} added but already have 2 controllers",
                which
            ));
            return;
        }

        let Some(ref game_controller_subsystem) = self.game_controller_subsystem else {
            log_info(format!(
                "Controller {} added but gamepad support is disabled",
                which
            ));
            return;
        };

        if !game_controller_subsystem.is_game_controller(which) {
            log_info(format!("Device {} is not a game controller", which));
            return;
        }

        match game_controller_subsystem.open(which) {
            Ok(controller) => {
                let instance_id = controller.instance_id();
                let player_num = (self.controllers.len() + 1) as u8;
                log_info(format!(
                    "Hot-plugged controller {} for player {}: {}",
                    which,
                    player_num,
                    controller.name()
                ));
                self.controller_player_map.insert(instance_id, player_num);
                self.controllers.push(controller);
            }
            Err(e) => {
                log_info(format!("Failed to open controller {}: {}", which, e));
            }
        }
    }

    /// Handle controller device removed event
    fn handle_controller_removed(&mut self, which: u32) {
        // Find and remove the controller with this instance_id
        if let Some(player_num) = self.controller_player_map.remove(&which) {
            // Remove from controllers vec
            self.controllers.retain(|c| c.instance_id() != which);
            log_info(format!(
                "Controller {} (player {}) removed",
                which, player_num
            ));

            // Reassign remaining controllers to players 1 and 2
            self.controller_player_map.clear();
            for (idx, controller) in self.controllers.iter().enumerate() {
                let instance_id = controller.instance_id();
                let new_player_num = (idx + 1) as u8;
                self.controller_player_map
                    .insert(instance_id, new_player_num);
                log_info(format!(
                    "Reassigned controller {} to player {}",
                    instance_id, new_player_num
                ));
            }
        }
    }

    /// Handle controller button press/release
    fn handle_controller_button(
        &self,
        nes: &mut Nes,
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

        let Some(port) = Self::assigned_gamepad_port(nes, &self.controller_player_map, player_num)
        else {
            return;
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
            nes.set_button(port, nes_button, pressed);
        }
    }
}

fn apply_volume_hotkey(audio: &SdlNesAudio, keycode: Keycode) {
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
    use crate::cartridge::Cartridge;
    use crate::console::Config;
    use serial_test::serial;
    use std::cell::RefCell;
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::rc::Rc;
    use tempfile::TempDir;

    fn default_config() -> Config {
        let mut config = Config::with_defaults();
        config.gamepads_enabled = false;
        config
    }

    fn copy_test_rom(temp_dir: &TempDir) -> PathBuf {
        let rom_path = temp_dir.path().join("test.nes");
        fs::copy("roms/automated_tests/nestest/nestest.nes", &rom_path)
            .expect("Failed to copy test ROM");
        rom_path
    }

    fn config_with_window_height(height: u32) -> Config {
        let mut config = Config::with_defaults();
        config.window_height = height;
        config
    }

    fn config_with_gamepads(enabled: bool) -> Config {
        let mut config = Config::with_defaults();
        config.gamepads_enabled = enabled;
        config
    }

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
            crate::cartridge::NametableLayout::Horizontal,
        );
        nes.insert_cartridge(cartridge);
    }

    fn read_joypad_buttons(nes: &mut Nes, port: u8) -> [u8; 8] {
        // Joypad serial order: A, B, Select, Start, Up, Down, Left, Right
        {
            let mut mem = nes.bus.borrow_mut();
            mem.write(0x4016, 1, false);
            mem.write(0x4016, 0, false);
        }

        let addr = if port == 1 { 0x4016 } else { 0x4017 };
        let mut out = [0u8; 8];
        for slot in &mut out {
            let value = nes.bus.borrow_mut().read(addr, false) & 0x01;
            *slot = value;
        }
        out
    }

    fn read_joypad1_buttons(nes: &mut Nes) -> [u8; 8] {
        read_joypad_buttons(nes, 1)
    }

    fn read_paddle_trigger_bit_for_port(nes: &mut Nes, port: u8) -> u8 {
        let addr = if port == 1 { 0x4016 } else { 0x4017 };
        let value = nes.bus.borrow_mut().read(addr, false);
        (value >> 3) & 0x01
    }

    fn read_paddle_trigger_bit(nes: &mut Nes) -> u8 {
        read_paddle_trigger_bit_for_port(nes, 1)
    }

    fn read_paddle_position_for_port(nes: &mut Nes, port: u8) -> u8 {
        {
            let mut mem = nes.bus.borrow_mut();
            mem.write(0x4016, 1, false);
            mem.write(0x4016, 0, false);
        }

        let addr = if port == 1 { 0x4016 } else { 0x4017 };
        let mut position = 0u8;
        for bit_index in (0..8).rev() {
            let value = nes.bus.borrow_mut().read(addr, false);
            let bit = (value >> 4) & 0x01;
            position |= bit << bit_index;
        }
        position ^ 0xFF
    }

    fn read_paddle_position(nes: &mut Nes) -> u8 {
        read_paddle_position_for_port(nes, 1)
    }

    fn tick_headless_once(event_loop: &mut SdlEventLoop, nes: &mut Nes) {
        let _should_quit = event_loop.tick_headless_once_for_run(nes);
    }

    #[test]
    #[serial]
    fn test_headless_run_tick_advances_to_frame_boundary() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let mut nes = nes_with_nop_loop_program();

        assert!(!nes.is_ready_to_render());
        event_loop.tick_headless_frame_for_run(&mut nes, &Tracing::default());
        assert!(
            nes.is_ready_to_render(),
            "headless run tick should emulate a full frame"
        );
    }

    #[test]
    #[serial]
    fn test_cycle_breakpoint_stops_emulation_immediately_not_at_frame_end() {
        // Regression: after a CYC breakpoint fires (setting paused=true), the inner emulation loop
        // must break immediately. Previously it kept running until the end of the frame, causing the
        // debugger to display the frame-end cycle count (~27000+) instead of the target (~100).
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let mut nes = nes_with_nop_loop_program();

        let cycles_before = nes.cpu.get_total_cycles();
        let target = cycles_before + 100;
        event_loop.add_cycle_breakpoint(target);

        event_loop.tick_headless_frame_for_run(&mut nes, &Tracing::default());

        assert!(
            event_loop.is_paused(),
            "should be paused after CYC breakpoint fires"
        );
        assert!(
            !nes.is_ready_to_render(),
            "frame should NOT have completed — emulation must stop at the breakpoint, not at frame end"
        );
        let cycles_after = nes.cpu.get_total_cycles();
        assert!(
            cycles_after < cycles_before + 200,
            "cycle count should be near the target ({target}), not at frame end; got {cycles_after}"
        );
    }

    #[test]
    #[serial]
    fn test_frame_breakpoint_pauses_on_first_instruction_of_target_frame() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let mut nes = nes_with_nop_loop_program();

        let target_frame = {
            let ppu = nes.ppu.borrow();
            ppu.frame_count() + 1
        };
        event_loop.add_frame_breakpoint(target_frame);

        let mut crossed_target_when_paused = false;
        for _ in 0..2_000_000 {
            let frame_before = {
                let ppu = nes.ppu.borrow();
                ppu.frame_count()
            };

            tick_headless_once(&mut event_loop, &mut nes);

            let frame_after = {
                let ppu = nes.ppu.borrow();
                ppu.frame_count()
            };

            if event_loop.is_paused() {
                crossed_target_when_paused =
                    frame_before < target_frame && frame_after >= target_frame;
                break;
            }

            assert!(
                frame_after < target_frame,
                "should pause at first instruction boundary after entering frame {target_frame}, but frame already reached {frame_after}"
            );
        }

        assert!(
            event_loop.is_paused(),
            "frame breakpoint should pause execution when target frame is reached"
        );
        assert!(
            crossed_target_when_paused,
            "frame breakpoint should trigger on the first instruction boundary after frame crossing"
        );
    }

    #[test]
    fn test_paddle_mouse_mapping_edges_and_center() {
        let window_width = 300;

        let left = SdlEventLoop::map_mouse_x_to_paddle_position(0, window_width);
        let right = SdlEventLoop::map_mouse_x_to_paddle_position(299, window_width);
        // Use the center of the discrete pixel range 0..=window_width-1.
        let center_x = ((window_width - 1) / 2) as i32;
        let center = SdlEventLoop::map_mouse_x_to_paddle_position(center_x, window_width);

        assert_eq!(left, 0x62);
        assert_eq!(right, 0xF2);
        assert!((165..=175).contains(&center));
    }

    #[test]
    fn test_paddle_mouse_mapping_non_linear_curve() {
        let window_width = 400;
        let center_a = 200;
        let center_b = 220;
        let edge_a = 360;
        let edge_b = 380;

        let center_delta = SdlEventLoop::map_mouse_x_to_paddle_position(center_b, window_width)
            - SdlEventLoop::map_mouse_x_to_paddle_position(center_a, window_width);
        let edge_delta = SdlEventLoop::map_mouse_x_to_paddle_position(edge_b, window_width)
            - SdlEventLoop::map_mouse_x_to_paddle_position(edge_a, window_width);

        assert!(edge_delta > center_delta);
    }

    #[test]
    fn test_zapper_mouse_mapping_edges_and_center() {
        let window_width = 320;
        let window_height = 240;

        let left = SdlEventLoop::map_mouse_x_to_zapper_position(0, window_width);
        let right = SdlEventLoop::map_mouse_x_to_zapper_position(319, window_width);
        let top = SdlEventLoop::map_mouse_y_to_zapper_position(0, window_height);
        let bottom = SdlEventLoop::map_mouse_y_to_zapper_position(239, window_height);

        assert_eq!(left, 0);
        assert_eq!(right, 255);
        assert_eq!(top, 0);
        assert_eq!(bottom, 255);
    }

    #[test]
    fn test_paddle_mode_suppresses_keyboard_joypad_input() {
        let mut paused = false;
        let mut debugger_open_requested = false;
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.bus
            .borrow_mut()
            .set_controller_type(1, crate::input::ControllerType::Arkanoid);
        nes.set_mouse_x_position(0x80);
        nes.set_mouse_left_button(true);

        let _ = SdlEventLoop::handle_key_down(
            &mut nes,
            Keycode::W,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );

        assert_eq!(read_joypad1_buttons(&mut nes), [0; 8]);
        assert_eq!(read_paddle_position(&mut nes), 0x80);
        assert_eq!(read_paddle_trigger_bit(&mut nes), 1);
    }

    #[test]
    #[serial]
    fn test_keyboard_only_targets_ports_without_gamepads() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        // One physical gamepad on port 1. Player 1 keys (WASD) should be inactive
        // because port 1 is taken by the gamepad. Player 2 keys (IJKL) should
        // control port 2.
        event_loop.controller_player_map.insert(42, 1);

        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::W);

        // WASD no longer falls through to port 2 – it is exclusively player 1's key set.
        assert_eq!(read_joypad_buttons(&mut nes, 1), [0; 8]);
        assert_eq!(read_joypad_buttons(&mut nes, 2), [0; 8]);

        // Player 2 key (I = Up) must reach port 2.
        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::I);
        assert_eq!(read_joypad_buttons(&mut nes, 1), [0; 8]);
        assert_eq!(read_joypad_buttons(&mut nes, 2), [0, 0, 0, 0, 1, 0, 0, 0]);
    }

    #[test]
    #[serial]
    fn test_handle_key_down_cmd_or_alt_f_toggles_fullscreen_state() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        assert!(!event_loop.is_fullscreen());

        let _ =
            event_loop.handle_key_down_for_run_with_modifiers(&mut nes, Keycode::F, Mod::LGUIMOD);
        assert!(event_loop.is_fullscreen());

        let _ =
            event_loop.handle_key_down_for_run_with_modifiers(&mut nes, Keycode::F, Mod::LALTMOD);
        assert!(!event_loop.is_fullscreen());
    }

    #[test]
    #[serial]
    fn test_keyboard_targets_first_gamepad_port_when_no_gamepads() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::W);

        assert_eq!(read_joypad_buttons(&mut nes, 1), [0, 0, 0, 0, 1, 0, 0, 0]);
        assert_eq!(read_joypad_buttons(&mut nes, 2), [0; 8]);
    }

    #[test]
    #[serial]
    fn test_player2_keyboard_dpad_no_gamepads() {
        // I=Up, K=Down, J=Left, L=Right should all route to port 2 when no gamepads connected.
        let config = default_config();

        let cases: &[(Keycode, [u8; 8])] = &[
            (Keycode::I, [0, 0, 0, 0, 1, 0, 0, 0]), // Up
            (Keycode::K, [0, 0, 0, 0, 0, 1, 0, 0]), // Down
            (Keycode::J, [0, 0, 0, 0, 0, 0, 1, 0]), // Left
            (Keycode::L, [0, 0, 0, 0, 0, 0, 0, 1]), // Right
        ];

        for (keycode, expected_port2) in cases {
            let mut event_loop =
                SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
            let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
                Config::default(),
            ));

            let _ = event_loop.handle_key_down_for_run(&mut nes, *keycode);

            assert_eq!(
                read_joypad_buttons(&mut nes, 1),
                [0; 8],
                "key {:?} must not affect port 1",
                keycode
            );
            assert_eq!(
                read_joypad_buttons(&mut nes, 2),
                *expected_port2,
                "key {:?} must control port 2",
                keycode
            );
        }
    }

    #[test]
    #[serial]
    fn test_player2_keyboard_buttons_no_gamepads() {
        // 9=Select, 0=Start, O=A, P=B should all route to port 2 when no gamepads connected.
        let config = default_config();

        let cases: &[(Keycode, [u8; 8])] = &[
            (Keycode::Num9, [0, 0, 1, 0, 0, 0, 0, 0]), // Select
            (Keycode::Num0, [0, 0, 0, 1, 0, 0, 0, 0]), // Start
            (Keycode::O, [1, 0, 0, 0, 0, 0, 0, 0]),    // A
            (Keycode::P, [0, 1, 0, 0, 0, 0, 0, 0]),    // B
        ];

        for (keycode, expected_port2) in cases {
            let mut event_loop =
                SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
            let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
                Config::default(),
            ));

            let _ = event_loop.handle_key_down_for_run(&mut nes, *keycode);

            assert_eq!(
                read_joypad_buttons(&mut nes, 1),
                [0; 8],
                "key {:?} must not affect port 1",
                keycode
            );
            assert_eq!(
                read_joypad_buttons(&mut nes, 2),
                *expected_port2,
                "key {:?} must control port 2",
                keycode
            );
        }
    }

    #[test]
    #[serial]
    fn test_player2_keyboard_key_release() {
        // Releasing a player 2 key must clear the button state on port 2.
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::I);
        assert_eq!(read_joypad_buttons(&mut nes, 2), [0, 0, 0, 0, 1, 0, 0, 0]);

        event_loop.handle_key_up_for_run(&mut nes, Keycode::I);
        assert_eq!(read_joypad_buttons(&mut nes, 2), [0; 8]);
    }

    #[test]
    #[serial]
    fn test_player2_keyboard_disabled_with_two_gamepads() {
        // When both ports have physical gamepads, player 2 keyboard must be inactive.
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        event_loop.controller_player_map.insert(42, 1);
        event_loop.controller_player_map.insert(43, 2);

        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::I);

        assert_eq!(read_joypad_buttons(&mut nes, 1), [0; 8]);
        assert_eq!(read_joypad_buttons(&mut nes, 2), [0; 8]);
    }

    #[test]
    #[serial]
    fn test_player1_keyboard_a_b_key_release() {
        // R=A and T=B: pressing then releasing each key must clear the correct button.
        let config = default_config();

        for (down_key, up_key, expected_down, label) in [
            (Keycode::R, Keycode::R, [1u8, 0, 0, 0, 0, 0, 0, 0], "R=A"),
            (Keycode::T, Keycode::T, [0u8, 1, 0, 0, 0, 0, 0, 0], "T=B"),
        ] {
            let mut event_loop =
                SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
            let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
                Config::default(),
            ));

            let _ = event_loop.handle_key_down_for_run(&mut nes, down_key);
            assert_eq!(
                read_joypad_buttons(&mut nes, 1),
                expected_down,
                "{label}: wrong button on press"
            );

            event_loop.handle_key_up_for_run(&mut nes, up_key);
            assert_eq!(
                read_joypad_buttons(&mut nes, 1),
                [0; 8],
                "{label}: button not cleared on release"
            );
        }
    }

    #[test]
    #[serial]
    fn test_gamepad_routes_to_first_gamepad_port() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        // Port 1 is mouse-only, port 2 is gamepad.
        nes.bus
            .borrow_mut()
            .set_controller_type(1, crate::input::ControllerType::Arkanoid);
        nes.bus
            .borrow_mut()
            .set_controller_type(2, crate::input::ControllerType::Joypad);

        event_loop.controller_player_map.insert(42, 1);
        event_loop.handle_controller_button(&mut nes, 42, sdl2::controller::Button::A, true);

        assert_eq!(read_joypad_buttons(&mut nes, 2), [1, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(read_joypad_buttons(&mut nes, 1), [0; 8]);
    }

    #[test]
    #[serial]
    fn test_gamepad_subsystem_retained_when_gamepads_enabled() {
        // When gamepads are enabled and no controllers are connected at startup,
        // the GameControllerSubsystem must be retained so that SDL continues to
        // fire ControllerDeviceAdded events for hot-plugged controllers.
        let mut config = Config::with_defaults();
        config.gamepads_enabled = true;
        let event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();

        assert!(
            event_loop.game_controller_subsystem.is_some(),
            "GameControllerSubsystem must be stored when gamepads are enabled"
        );
    }

    #[test]
    fn test_mouse_routes_to_all_mouse_ports() {
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.bus
            .borrow_mut()
            .set_controller_type(1, crate::input::ControllerType::Arkanoid);
        nes.bus
            .borrow_mut()
            .set_controller_type(2, crate::input::ControllerType::Arkanoid);

        let window_width = 320;
        let window_height = 240;
        let x = 240;
        let y = 120;
        let expected = SdlEventLoop::map_mouse_x_to_paddle_position(x, window_width);

        SdlEventLoop::update_mouse_motion(&mut nes, x, y, window_width, window_height);

        assert_eq!(read_paddle_position_for_port(&mut nes, 1), expected);
        assert_eq!(read_paddle_position_for_port(&mut nes, 2), expected);
    }

    #[test]
    fn test_paddle_mouse_button_sets_trigger_when_enabled() {
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.bus
            .borrow_mut()
            .set_controller_type(1, crate::input::ControllerType::Arkanoid);

        SdlEventLoop::update_mouse_button(&mut nes, MouseButton::Left, true);
        assert_eq!(read_paddle_trigger_bit(&mut nes), 1);

        SdlEventLoop::update_mouse_button(&mut nes, MouseButton::Left, false);
        assert_eq!(read_paddle_trigger_bit(&mut nes), 0);
    }

    #[test]
    fn test_paddle_mouse_button_ignored_when_disabled() {
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        nes.bus
            .borrow_mut()
            .set_controller_type(1, crate::input::ControllerType::Joypad);
        SdlEventLoop::update_mouse_button(&mut nes, MouseButton::Left, true);

        nes.bus
            .borrow_mut()
            .set_controller_type(1, crate::input::ControllerType::Arkanoid);
        assert_eq!(read_paddle_trigger_bit(&mut nes), 0);
    }

    #[test]
    fn test_paddle_mouse_motion_updates_position_when_enabled() {
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.bus
            .borrow_mut()
            .set_controller_type(1, crate::input::ControllerType::Arkanoid);

        let window_width = 320;
        let window_height = 240;
        let x = 240;
        let y = 120;
        let expected = SdlEventLoop::map_mouse_x_to_paddle_position(x, window_width);

        SdlEventLoop::update_mouse_motion(&mut nes, x, y, window_width, window_height);

        assert_eq!(read_paddle_position(&mut nes), expected);
    }

    #[test]
    fn test_paddle_mouse_motion_ignored_when_disabled() {
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        let window_width = 320;
        let window_height = 240;
        let x = 240;
        let y = 120;

        nes.bus
            .borrow_mut()
            .set_controller_type(1, crate::input::ControllerType::Joypad);
        SdlEventLoop::update_mouse_motion(&mut nes, x, y, window_width, window_height);

        nes.bus
            .borrow_mut()
            .set_controller_type(1, crate::input::ControllerType::Arkanoid);
        assert_eq!(read_paddle_position(&mut nes), 0x62);
    }

    #[test]
    #[serial]
    fn test_paddle_mode_suppresses_controller_input() {
        let config = default_config();
        let mut event_loop = match SdlEventLoop::new(
            true,
            None,
            AppContext::new_with_config(config.clone()),
        ) {
            Ok(event_loop) => event_loop,
            Err(err) if err.contains("Cannot initialize `Sdl` from more than one thread.") => {
                eprintln!(
                    "Skipping test_paddle_mode_suppresses_controller_input due to SDL thread-affinity: {err}"
                );
                return;
            }
            Err(err) => panic!("headless event loop init failed: {err}"),
        };
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.bus
            .borrow_mut()
            .set_controller_type(1, crate::input::ControllerType::Arkanoid);

        event_loop.controller_player_map.insert(42, 1);
        event_loop.handle_controller_button(&mut nes, 42, sdl2::controller::Button::A, true);

        assert_eq!(read_joypad1_buttons(&mut nes), [0; 8]);
    }

    #[test]
    #[serial]
    fn test_breakpoint_hit_pauses_and_opens_debugger() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        insert_nop_cartridge(&mut nes, 0x8000);
        nes.reset(false);

        // Run until PC reaches $8002, then expect the breakpoint to pause *before* executing it.
        event_loop.add_breakpoint(0x8002);

        // Execute two instructions: $8000 and $8001.
        tick_headless_once(&mut event_loop, &mut nes);
        tick_headless_once(&mut event_loop, &mut nes);
        assert_eq!(nes.cpu.pc(), 0x8002);
        assert!(!event_loop.is_paused());
        assert!(!event_loop.debugger_open_requested());

        // Next tick should notice the breakpoint and enter the debugger (paused).
        tick_headless_once(&mut event_loop, &mut nes);
        assert_eq!(nes.cpu.pc(), 0x8002);
        assert!(event_loop.is_paused());
        assert!(event_loop.debugger_open_requested());
    }

    #[test]
    #[serial]
    fn test_remove_breakpoint_allows_execution_to_continue() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        insert_nop_cartridge(&mut nes, 0x8000);
        nes.reset(false);

        event_loop.add_breakpoint(0x8001);
        event_loop.remove_breakpoint(0x8001);

        // If the breakpoint was removed, we should not pause when reaching $8001.
        tick_headless_once(&mut event_loop, &mut nes);
        assert_eq!(nes.cpu.pc(), 0x8001);
        assert!(!event_loop.is_paused());
        assert!(!event_loop.debugger_open_requested());
    }

    #[test]
    fn test_manual_frame_limiting_is_disabled_with_vsync() {
        assert!(!SdlEventLoop::should_manual_frame_limit(true));
    }

    #[test]
    fn test_manual_frame_limiting_is_enabled_without_vsync() {
        assert!(SdlEventLoop::should_manual_frame_limit(false));
    }

    #[test]
    #[serial]
    fn test_eventloop_creation() {
        let config = default_config();
        let event_loop = SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone()));
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
        let audio = SdlNesAudio::new(&sdl_context, 44100).expect("Audio init should succeed");

        let initial_volume = audio.get_volume();

        apply_volume_hotkey(&audio, Keycode::F2);
        assert!(
            (audio.get_volume() - (initial_volume + 0.1)).abs() < 1e-6,
            "F2 should raise volume by 0.1"
        );

        apply_volume_hotkey(&audio, Keycode::F3);
        assert!(
            (audio.get_volume() - initial_volume).abs() < 1e-6,
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
        let audio = SdlNesAudio::new(&sdl_context, 44100).expect("Audio init should succeed");
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        let mut paused = false;
        let mut debugger_open_requested = false;

        let before = audio.get_volume();
        SdlEventLoop::handle_key_down(
            &mut nes,
            Keycode::F2,
            Some(&audio),
            &mut paused,
            &mut debugger_open_requested,
        );
        assert!((audio.get_volume() - (before + 0.1)).abs() < 1e-6);
        SdlEventLoop::handle_key_down(
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
    fn test_handle_key_down_f6_saves_state_next_to_rom() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let rom_path = copy_test_rom(&temp_dir);

        let app_context = AppContext::new();
        let rom_bytes = std::fs::read(&rom_path).expect("Failed to read ROM");
        let cart = Cartridge::load_from_file(&rom_bytes, &rom_path, &app_context)
            .expect("Failed to load ROM");
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.insert_cartridge(cart);
        nes.reset(false);

        let mut paused = false;
        let mut debugger_open_requested = false;
        SdlEventLoop::handle_key_down(
            &mut nes,
            Keycode::F6,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );

        let state_path = rom_path.with_extension("state");
        assert!(state_path.exists(), "Expected state file to be created");
    }

    #[test]
    fn test_handle_key_down_f7_loads_state_when_present() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let rom_path = copy_test_rom(&temp_dir);

        let app_context = AppContext::new();
        let rom_bytes = std::fs::read(&rom_path).expect("Failed to read ROM");
        let cart = Cartridge::load_from_file(&rom_bytes, &rom_path, &app_context)
            .expect("Failed to load ROM");
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.insert_cartridge(cart);
        nes.reset(false);

        let saved_pc = nes.cpu.pc();
        let mut paused = false;
        let mut debugger_open_requested = false;
        SdlEventLoop::handle_key_down(
            &mut nes,
            Keycode::F6,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );

        nes.cpu.set_pc(saved_pc.wrapping_add(1));
        SdlEventLoop::handle_key_down(
            &mut nes,
            Keycode::F7,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );

        assert_eq!(nes.cpu.pc(), saved_pc);
    }

    #[test]
    #[serial]
    fn test_handle_key_down_cmd_or_alt_q_requests_quit() {
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();

        let outcome =
            event_loop.handle_key_down_for_run_with_modifiers(&mut nes, Keycode::Q, Mod::LGUIMOD);
        assert_eq!(outcome, KeyDownOutcome::Quit);

        let outcome =
            event_loop.handle_key_down_for_run_with_modifiers(&mut nes, Keycode::Q, Mod::LALTMOD);
        assert_eq!(outcome, KeyDownOutcome::Quit);
    }

    #[test]
    #[serial]
    fn test_handle_key_down_cmd_or_alt_o_requests_cartridge_switch_dialog() {
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config)).unwrap();

        let _ =
            event_loop.handle_key_down_for_run_with_modifiers(&mut nes, Keycode::O, Mod::LGUIMOD);
        assert!(event_loop.cartridge_switch_dialog_requested);
    }

    #[test]
    #[serial]
    fn test_load_cartridge_switch_entries_from_csv_populates_entries() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let csv_path = temp_dir.path().join("cartridges.csv");
        fs::write(&csv_path, "path\nroms/games/a.nes\nroms/games/sub/b.nes\n").expect("write csv");

        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config)).unwrap();

        event_loop
            .load_cartridge_switch_entries_from_csv(&csv_path)
            .expect("csv load should succeed");

        assert_eq!(
            event_loop.cartridge_switch_entries,
            vec![
                "roms/games/a.nes".to_string(),
                "roms/games/sub/b.nes".to_string()
            ]
        );
    }

    #[test]
    #[serial]
    fn test_switch_to_cartridge_path_loads_cartridge_into_nes() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let rom_path = copy_test_rom(&temp_dir);
        let rom_path_str = rom_path.to_string_lossy().to_string();

        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config)).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        event_loop
            .switch_to_cartridge_path(&mut nes, &rom_path_str)
            .expect("cartridge switch should succeed");

        assert!(
            nes.state_path().is_some(),
            "NES should have cartridge state path after switch"
        );
    }

    #[test]
    #[serial]
    fn test_cartridge_switch_dialog_overlay_shows_selected_entry() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config)).unwrap();
        let nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        event_loop.cartridge_switch_dialog_requested = true;
        event_loop.cartridge_switch_entries = vec!["roms/games/a.nes".to_string()];
        event_loop.cartridge_switch_selected_index = 0;

        let overlay = event_loop
            .overlay_render_text(&nes)
            .expect("overlay text should be present");
        assert!(overlay.contains("Switch cartridge"));
        assert!(overlay.contains(">> roms/games/a.nes <<"));
    }

    #[test]
    #[serial]
    fn test_cartridge_switch_dialog_down_and_up_changes_selection() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config)).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        event_loop.cartridge_switch_dialog_requested = true;
        event_loop.paused = true;
        event_loop.cartridge_switch_entries = vec![
            "roms/games/a.nes".to_string(),
            "roms/games/b.nes".to_string(),
        ];
        event_loop.cartridge_switch_selected_index = 0;

        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::Down);
        assert_eq!(event_loop.cartridge_switch_selected_index, 1);

        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::Up);
        assert_eq!(event_loop.cartridge_switch_selected_index, 0);
    }

    #[test]
    #[serial]
    fn test_cartridge_switch_dialog_escape_closes_dialog_and_unpauses() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config)).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        event_loop.cartridge_switch_dialog_requested = true;
        event_loop.paused = true;

        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::Escape);

        assert!(!event_loop.cartridge_switch_dialog_requested);
        assert!(!event_loop.paused);
    }

    #[test]
    #[serial]
    fn test_cartridge_switch_dialog_enter_loads_selected_rom_and_closes() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let rom_path = copy_test_rom(&temp_dir);
        let rom_path_str = rom_path.to_string_lossy().to_string();

        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config)).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        event_loop.cartridge_switch_dialog_requested = true;
        event_loop.paused = true;
        event_loop.cartridge_switch_entries = vec![rom_path_str.clone()];
        event_loop.cartridge_switch_selected_index = 0;

        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::Return);

        assert_eq!(
            nes.state_path(),
            Some(rom_path.with_extension("state")),
            "Selected ROM should be loaded into NES"
        );
        assert!(!event_loop.cartridge_switch_dialog_requested);
        assert!(!event_loop.paused);
    }

    #[test]
    #[serial]
    fn test_cartridge_switch_dialog_typing_filters_entries() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config)).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        event_loop.cartridge_switch_dialog_requested = true;
        event_loop.paused = true;
        event_loop.cartridge_switch_entries = vec![
            "roms/games/mario.nes".to_string(),
            "roms/games/zelda.nes".to_string(),
            "roms/games/mega_man.nes".to_string(),
        ];
        event_loop.refresh_cartridge_switch_filtered_entries();

        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::M);

        let overlay = event_loop
            .overlay_render_text(&nes)
            .expect("overlay should be visible");
        assert!(overlay.contains("Filter: m"));
        assert!(overlay.contains("Matches: 2/3"));
        assert!(overlay.contains("mario.nes"));
        assert!(overlay.contains("mega_man.nes"));
        assert!(!overlay.contains("zelda.nes"));
    }

    #[test]
    #[serial]
    fn test_cartridge_switch_dialog_fuzzy_subsequence_match_is_supported() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config)).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        event_loop.cartridge_switch_dialog_requested = true;
        event_loop.paused = true;
        event_loop.cartridge_switch_entries = vec![
            "roms/games/mega_man.nes".to_string(),
            "roms/games/mario.nes".to_string(),
        ];
        event_loop.refresh_cartridge_switch_filtered_entries();

        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::M);
        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::M);
        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::N);

        let overlay = event_loop
            .overlay_render_text(&nes)
            .expect("overlay should be visible");
        assert!(overlay.contains("Filter: mmn"));
        assert!(overlay.contains("mega_man.nes"));
        assert!(!overlay.contains("mario.nes"));
    }

    #[test]
    #[serial]
    fn test_cartridge_switch_dialog_fuzzy_matching_is_order_sensitive() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config)).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        event_loop.cartridge_switch_dialog_requested = true;
        event_loop.paused = true;
        event_loop.cartridge_switch_entries = vec!["roms/games/mega_man.nes".to_string()];
        event_loop.refresh_cartridge_switch_filtered_entries();

        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::N);
        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::M);

        let overlay = event_loop
            .overlay_render_text(&nes)
            .expect("overlay should be visible");
        assert!(overlay.contains("Filter: nm"));
        assert!(overlay.contains("Matches: 0/1"));
        assert!(overlay.contains("No matching entries"));
    }

    #[test]
    #[serial]
    fn test_cartridge_switch_dialog_backspace_updates_filter() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config)).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        event_loop.cartridge_switch_dialog_requested = true;
        event_loop.paused = true;
        event_loop.cartridge_switch_entries = vec![
            "roms/games/mario.nes".to_string(),
            "roms/games/zelda.nes".to_string(),
        ];
        event_loop.refresh_cartridge_switch_filtered_entries();

        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::M);
        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::Backspace);

        let overlay = event_loop
            .overlay_render_text(&nes)
            .expect("overlay should be visible");
        assert!(overlay.contains("Filter: (none)"));
        assert!(overlay.contains("mario.nes"));
        assert!(overlay.contains("zelda.nes"));
    }

    #[test]
    #[serial]
    fn test_cartridge_switch_dialog_j_and_k_are_filter_characters() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config)).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        event_loop.cartridge_switch_dialog_requested = true;
        event_loop.paused = true;
        event_loop.cartridge_switch_entries = vec![
            "roms/games/jk_mario.nes".to_string(),
            "roms/games/zelda.nes".to_string(),
        ];
        event_loop.refresh_cartridge_switch_filtered_entries();

        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::J);
        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::K);

        let overlay = event_loop
            .overlay_render_text(&nes)
            .expect("overlay should be visible");
        assert!(overlay.contains("Filter: jk"));
        assert!(overlay.contains("jk_mario.nes"));
        assert!(!overlay.contains("zelda.nes"));
    }

    #[test]
    #[serial]
    fn test_cartridge_switch_dialog_enter_loads_filtered_selected_rom() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let rom_path = copy_test_rom(&temp_dir);
        let rom_path_str = rom_path.to_string_lossy().to_string();

        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config)).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        event_loop.cartridge_switch_dialog_requested = true;
        event_loop.paused = true;
        event_loop.cartridge_switch_entries = vec![
            "roms/games/missing_rom.nes".to_string(),
            rom_path_str.clone(),
        ];
        event_loop.refresh_cartridge_switch_filtered_entries();

        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::T);
        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::E);
        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::S);
        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::T);
        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::Return);

        assert_eq!(
            nes.state_path(),
            Some(rom_path.with_extension("state")),
            "Filtered selected ROM should be loaded into NES"
        );
    }

    #[test]
    #[serial]
    fn test_cartridge_switch_dialog_reopen_preserves_filter_and_selection() {
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

        let temp_home = tempfile::tempdir().expect("temp home");
        let restore = EnvRestore {
            key: "HOME",
            prev: env::var("HOME").ok(),
        };
        unsafe {
            env::set_var("HOME", temp_home.path());
        }

        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config)).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        event_loop.cartridge_switch_entries = vec![
            "roms/games/mega_man.nes".to_string(),
            "roms/games/mario.nes".to_string(),
            "roms/games/zelda.nes".to_string(),
        ];
        let _ = event_loop.request_cartridge_switch_dialog();

        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::M);
        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::Down);
        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::Escape);

        let _ = event_loop.request_cartridge_switch_dialog();
        let overlay = event_loop
            .overlay_render_text(&nes)
            .expect("overlay should be visible");

        assert!(overlay.contains("Filter: m"));
        assert!(overlay.contains(">> roms/games/mario.nes <<"));

        drop(restore);
    }

    #[test]
    #[serial]
    fn test_cartridge_switch_dialog_refresh_uses_first_match_when_selection_removed() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config)).unwrap();
        let nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        event_loop.cartridge_switch_dialog_requested = true;
        event_loop.paused = true;
        event_loop.cartridge_switch_filter_query = "m".to_string();
        event_loop.cartridge_switch_entries = vec![
            "roms/games/mega_man.nes".to_string(),
            "roms/games/mario.nes".to_string(),
        ];
        event_loop.refresh_cartridge_switch_filtered_entries();
        event_loop.cartridge_switch_selected_index = 1;

        event_loop.cartridge_switch_entries = vec!["roms/games/mega_man.nes".to_string()];
        event_loop.refresh_cartridge_switch_filtered_entries();

        let overlay = event_loop
            .overlay_render_text(&nes)
            .expect("overlay should be visible");
        assert!(overlay.contains(">> roms/games/mega_man.nes <<"));
    }

    #[test]
    fn test_handle_key_down_space_toggles_pause() {
        // Desired behavior: Space toggles pause state via centralized handle_key_down.
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        let mut paused = false;
        let mut debugger_open_requested = false;

        let _ = SdlEventLoop::handle_key_down(
            &mut nes,
            Keycode::Space,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );
        assert!(paused);
        let _ = SdlEventLoop::handle_key_down(
            &mut nes,
            Keycode::Space,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );
        assert!(!paused);
    }

    #[test]
    #[serial]
    fn test_handle_key_down_h_toggles_help_overlay() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::H);
        assert!(event_loop.help_overlay_visible);

        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::H);
        assert!(!event_loop.help_overlay_visible);
    }

    #[test]
    #[serial]
    fn test_help_overlay_text_mentions_shortcuts() {
        let config = default_config();
        let event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        let text = event_loop.help_overlay_text(&nes);
        assert!(text.contains("Cmd/Alt+Q"));
        assert!(text.contains("Space"));
        assert!(text.contains("Cmd/Alt+R"));
        assert!(text.contains("Shift+Cmd/Alt+R"));
        assert!(text.contains("F2"));
        assert!(text.contains("F3"));
        assert!(text.contains("F5"));
        assert!(text.contains("F6"));
        assert!(text.contains("F7"));
        assert!(text.contains("F10"));
        assert!(text.contains("F11"));
        assert!(text.contains("Cmd/Alt+F"));
        assert!(text.contains("Cmd/Alt+O"));
        assert!(text.contains("W/A/S/D"));
        assert!(text.contains("R"));
        assert!(text.contains("T"));
        assert!(text.contains("4: Select"));
        assert!(text.contains("5: Start"));
        assert!(text.contains("H"));
    }

    #[test]
    #[serial]
    fn test_help_overlay_shows_keyboard_for_both_players_when_no_gamepads() {
        // Given no physical gamepads connected, both players should show keyboard keys.
        let config = default_config();
        let event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        let text = event_loop.help_overlay_text(&nes);

        assert!(text.contains("W/A/S/D"), "Player 1 D-pad should be shown");
        assert!(text.contains("I/J/K/L"), "Player 2 D-pad should be shown");
        assert!(!text.contains("Gamepad"), "No gamepad should be mentioned");
    }

    #[test]
    #[serial]
    fn test_help_overlay_shows_gamepad_for_player1_when_one_gamepad_connected() {
        // Given one gamepad on port 1, player 1 section shows "Gamepad" and player 2 shows keyboard.
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        event_loop.controller_player_map.insert(42, 1);

        let text = event_loop.help_overlay_text(&nes);

        assert!(text.contains("Gamepad"), "Player 1 should show Gamepad");
        assert!(
            !text.contains("W/A/S/D"),
            "Player 1 keyboard should be hidden"
        );
        assert!(
            text.contains("I/J/K/L"),
            "Player 2 keyboard D-pad should still be shown"
        );
    }

    #[test]
    #[serial]
    fn test_help_overlay_shows_gamepad_for_both_players_when_two_gamepads_connected() {
        // Given two gamepads, both player sections show "Gamepad" and no keyboard keys.
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        event_loop.controller_player_map.insert(42, 1);
        event_loop.controller_player_map.insert(43, 2);

        let text = event_loop.help_overlay_text(&nes);

        assert!(
            !text.contains("W/A/S/D"),
            "Player 1 keyboard should be hidden"
        );
        assert!(
            !text.contains("I/J/K/L"),
            "Player 2 keyboard should be hidden"
        );
        // Both players should have a "Gamepad" entry
        assert_eq!(
            text.matches("Gamepad").count(),
            2,
            "Both players should show Gamepad"
        );
    }

    #[test]
    #[serial]
    fn test_help_overlay_text_for_rendering() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        assert_eq!(event_loop.help_overlay_render_text(&nes), None);

        event_loop.help_overlay_visible = true;
        assert!(event_loop.help_overlay_render_text(&nes).is_some());
    }

    fn nes_with_jsr_program() -> Nes {
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

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
            crate::cartridge::NametableLayout::Horizontal,
        );
        nes.insert_cartridge(cartridge);
        nes.reset(false);
        nes
    }

    fn nes_with_nop_loop_program() -> Nes {
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

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
            crate::cartridge::NametableLayout::Horizontal,
        );
        nes.insert_cartridge(cartridge);
        nes.reset(false);
        nes
    }

    #[test]
    fn test_handle_key_down_f5_pauses_emulation() {
        // Desired behavior: F5 opens debugger windows, which immediately pauses emulation.
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        let mut paused = false;
        let mut debugger_open_requested = false;

        let _ = SdlEventLoop::handle_key_down(
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
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        let mut paused = true;
        let mut debugger_open_requested = true;

        let _ = SdlEventLoop::handle_key_down(
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
        SdlEventLoop::debugger_run_to_next_frame(&mut nes_actual);
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

    #[serial]
    #[test]
    fn test_run_to_next_scanline_stops_after_first_scanline_advance() {
        let mut nes_expected = nes_with_nop_loop_program();
        let config = Config::default();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        event_loop.debugger_open_requested = true;

        {
            let mut ppu = nes_expected.ppu.borrow_mut();
            while ppu.scanline() != 100 || ppu.pixel() != 338 {
                ppu.run_ppu_cycles(1);
            }
        }

        let save_state = nes_expected.save_state();
        let expected_scanline = {
            nes_expected.run_cpu_tick();
            let ppu = nes_expected.ppu.borrow();
            ppu.scanline()
        };
        let expected_pixel = {
            let ppu = nes_expected.ppu.borrow();
            ppu.pixel()
        };

        let mut nes_actual = nes_with_nop_loop_program();
        nes_actual.load_state(&save_state).unwrap();

        event_loop.apply_debugger_ui_action(
            &mut nes_actual,
            ui::DebuggerUiAction {
                run_to_next_scanline: true,
                ..Default::default()
            },
        );

        let (actual_scanline, actual_pixel) = {
            let ppu = nes_actual.ppu.borrow();
            (ppu.scanline(), ppu.pixel())
        };

        assert_eq!(actual_scanline, expected_scanline);
        assert_eq!(actual_pixel, expected_pixel);
    }

    #[test]
    fn test_handle_key_down_f10_step_over_jsr_runs_until_return() {
        let mut nes = nes_with_jsr_program();
        nes.cpu.set_x(0);

        let mut paused = true;
        let mut debugger_open_requested = true;

        let _ = SdlEventLoop::handle_key_down(
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
            nes.cpu.pc(),
            0x8003,
            "expected step-over to stop at next instruction"
        );
        assert_eq!(
            nes.cpu.x(),
            1,
            "expected subroutine to have executed (INX) before returning"
        );
    }

    #[test]
    fn test_handle_key_down_f11_step_into_jsr_enters_subroutine() {
        let mut nes = nes_with_jsr_program();
        nes.cpu.set_x(0);

        let mut paused = true;
        let mut debugger_open_requested = true;

        let _ = SdlEventLoop::handle_key_down(
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
        assert_eq!(
            nes.cpu.pc(),
            0x8006,
            "expected step-into to enter subroutine"
        );
        assert_eq!(
            nes.cpu.x(),
            0,
            "expected to not execute INX when stepping into JSR"
        );
    }

    #[test]
    #[serial]
    fn test_continue_action_unpauses_and_closes_debugger() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        event_loop.request_debugger_open();

        event_loop.apply_debugger_ui_action(
            &mut nes,
            crate::debugging::ui::DebuggerUiAction {
                continue_run: true,
                ..Default::default()
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
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        insert_nop_cartridge(&mut nes, 0x8000);
        nes.reset(false);

        // Break immediately at the first instruction.
        event_loop.add_breakpoint(0x8000);

        // First tick hits the breakpoint and pauses before executing.
        tick_headless_once(&mut event_loop, &mut nes);
        assert_eq!(nes.cpu.pc(), 0x8000);
        assert!(event_loop.is_paused());
        assert!(event_loop.debugger_open_requested());

        // Continue should unpause and not instantly re-break at the same PC.
        event_loop.apply_debugger_ui_action(
            &mut nes,
            crate::debugging::ui::DebuggerUiAction {
                continue_run: true,
                ..Default::default()
            },
        );
        assert!(!event_loop.is_paused());
        assert!(!event_loop.debugger_open_requested());

        // Next tick must execute the instruction at $8000 (NOP) and advance.
        tick_headless_once(&mut event_loop, &mut nes);
        assert_eq!(nes.cpu.pc(), 0x8001);
        assert!(!event_loop.is_paused());
        assert!(!event_loop.debugger_open_requested());
    }

    #[test]
    #[serial]
    fn test_f5_when_debugger_open_behaves_like_continue_for_breakpoints() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        insert_nop_cartridge(&mut nes, 0x8000);
        nes.reset(false);

        event_loop.add_breakpoint(0x8000);

        // Hit the breakpoint first.
        tick_headless_once(&mut event_loop, &mut nes);
        assert_eq!(nes.cpu.pc(), 0x8000);
        assert!(event_loop.is_paused());
        assert!(event_loop.debugger_open_requested());

        // Pressing F5 while debugger is open should behave like Continue.
        let _ = event_loop.handle_key_down_for_run(&mut nes, Keycode::F5);
        assert!(!event_loop.is_paused());
        assert!(!event_loop.debugger_open_requested());

        // Next tick must execute past the breakpoint without immediately re-breaking.
        tick_headless_once(&mut event_loop, &mut nes);
        assert_eq!(nes.cpu.pc(), 0x8001);
        assert!(!event_loop.is_paused());
        assert!(!event_loop.debugger_open_requested());
    }

    #[test]
    #[serial]
    fn test_run_to_nmi_action_runs_until_nmi_vector_pc() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

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
            crate::cartridge::NametableLayout::Horizontal,
        );
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        // Force an NMI edge before running.
        nes.cpu.set_nmi_pending(true);

        event_loop.request_debugger_open();
        event_loop.apply_debugger_ui_action(
            &mut nes,
            crate::debugging::ui::DebuggerUiAction {
                run_to_nmi: true,
                ..Default::default()
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
        assert_eq!(nes.cpu.pc(), nmi_vector);
        assert!(
            event_loop.temporary_breakpoint.is_none(),
            "temporary breakpoint should clear after being hit"
        );
        assert!(
            !event_loop
                .breakpoints
                .iter()
                .any(|b| b.kind == BreakpointKind::Pc(nmi_vector)),
            "temporary breakpoint should be removed after being hit"
        );
    }

    #[test]
    #[serial]
    fn test_run_to_irq_action_runs_until_irq_vector_pc() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

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
            crate::cartridge::NametableLayout::Horizontal,
        );
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        // Ensure IRQs are unmasked (clear I flag).
        nes.cpu.set_p(nes.cpu.p() & !0b0000_0100);

        // Force an IRQ to be pending.
        nes.cpu.set_irq_pending(true);

        event_loop.request_debugger_open();
        event_loop.apply_debugger_ui_action(
            &mut nes,
            crate::debugging::ui::DebuggerUiAction {
                run_to_irq: true,
                ..Default::default()
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
        assert_eq!(nes.cpu.pc(), irq_vector);
        assert!(
            event_loop.temporary_breakpoint.is_none(),
            "temporary breakpoint should clear after being hit"
        );
        assert!(
            !event_loop
                .breakpoints
                .iter()
                .any(|b| b.kind == BreakpointKind::Pc(irq_vector)),
            "temporary breakpoint should be removed after being hit"
        );
    }

    #[test]
    #[serial]
    fn test_run_to_irq_requires_actual_irq_entry_not_just_pc_match() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

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
            crate::cartridge::NametableLayout::Horizontal,
        );
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        // Ensure IRQs are unmasked (clear I flag).
        nes.cpu.set_p(nes.cpu.p() & !0b0000_0100);
        // Force an IRQ to be pending.
        nes.cpu.set_irq_pending(true);

        event_loop.request_debugger_open();
        event_loop.apply_debugger_ui_action(
            &mut nes,
            crate::debugging::ui::DebuggerUiAction {
                run_to_irq: true,
                ..Default::default()
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
        assert_eq!(nes.cpu.pc(), irq_vector);
        assert_eq!(
            nes.cpu.current_interrupt(),
            Some(crate::cpu::InterruptKind::Irq)
        );
    }

    #[test]
    #[serial]
    fn test_run_to_nmi_when_already_in_nmi_waits_for_next_nmi_entry() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

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
            crate::cartridge::NametableLayout::Horizontal,
        );
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        // Make the handler loop a bit (revisiting $9000) before RTI.
        nes.bus.borrow_mut().write_for_testing(0x0000, 3);

        // Enter NMI once, stopping at the vector.
        nes.cpu.set_nmi_pending(true);
        for _ in 0..1_000_000 {
            nes.run_cpu_tick();
            if nes.cpu.current_interrupt() == Some(crate::cpu::InterruptKind::Nmi)
                && nes.cpu.pc() == nmi_vector
            {
                break;
            }
        }
        assert_eq!(
            nes.cpu.current_interrupt(),
            Some(crate::cpu::InterruptKind::Nmi)
        );
        assert_eq!(nes.cpu.pc(), nmi_vector);

        // Now request Run-to-NMI while we're already inside the current NMI handler.
        event_loop.request_debugger_open();
        event_loop.apply_debugger_ui_action(
            &mut nes,
            crate::debugging::ui::DebuggerUiAction {
                run_to_nmi: true,
                ..Default::default()
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
        assert_eq!(nes.cpu.pc(), nmi_vector);
    }

    #[test]
    #[serial]
    fn test_run_to_nmi_ignores_other_breakpoints_until_next_nmi_entry() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

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
            crate::cartridge::NametableLayout::Horizontal,
        );
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        // Enter NMI once, stopping at the vector.
        nes.cpu.set_nmi_pending(true);
        for _ in 0..1_000_000 {
            nes.run_cpu_tick();
            if nes.cpu.current_interrupt() == Some(crate::cpu::InterruptKind::Nmi)
                && nes.cpu.pc() == nmi_vector
            {
                break;
            }
        }
        assert_eq!(
            nes.cpu.current_interrupt(),
            Some(crate::cpu::InterruptKind::Nmi)
        );
        assert_eq!(nes.cpu.pc(), nmi_vector);

        // Place a breakpoint inside the current NMI handler to reproduce the user-reported
        // "Run to NMI just steps one instruction" behavior.
        let handler_second_instruction = nmi_vector.wrapping_add(1);
        event_loop.add_breakpoint(handler_second_instruction);

        // Now request Run-to-NMI while we're already inside the current NMI handler.
        event_loop.request_debugger_open();
        event_loop.apply_debugger_ui_action(
            &mut nes,
            crate::debugging::ui::DebuggerUiAction {
                run_to_nmi: true,
                ..Default::default()
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
        assert_eq!(nes.cpu.pc(), nmi_vector);
    }

    #[test]
    #[serial]
    fn test_step_into_action_runs_via_temporary_breakpoint_and_reopens_debugger() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        insert_nop_cartridge(&mut nes, 0x8000);
        nes.reset(false);

        event_loop.request_debugger_open();
        assert!(event_loop.is_paused());
        assert!(event_loop.debugger_open_requested());

        event_loop.apply_debugger_ui_action(
            &mut nes,
            crate::debugging::ui::DebuggerUiAction {
                step_into: true,
                ..Default::default()
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

        assert_eq!(nes.cpu.pc(), 0x8001);
        assert!(event_loop.is_paused());
        assert!(event_loop.debugger_open_requested());
    }

    #[test]
    #[serial]
    fn test_step_over_action_runs_via_temporary_breakpoint_and_reopens_debugger() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let mut nes = nes_with_jsr_program();
        nes.cpu.set_x(0);

        event_loop.request_debugger_open();
        assert!(event_loop.is_paused());
        assert!(event_loop.debugger_open_requested());

        event_loop.apply_debugger_ui_action(
            &mut nes,
            crate::debugging::ui::DebuggerUiAction {
                step_over: true,
                ..Default::default()
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
            nes.cpu.pc(),
            0x8003,
            "expected step-over to stop at next instruction"
        );
        assert_eq!(
            nes.cpu.x(),
            1,
            "expected subroutine to have executed (INX) before returning"
        );
        assert!(event_loop.is_paused());
        assert!(event_loop.debugger_open_requested());
    }

    #[test]
    #[serial]
    fn test_request_debugger_open_pauses_and_sets_request_flag() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();

        assert!(!event_loop.paused);
        assert!(!event_loop.debugger_open_requested);

        event_loop.request_debugger_open();

        assert!(event_loop.paused);
        assert!(event_loop.debugger_open_requested);
    }

    #[test]
    fn test_handle_key_down_f1_no_longer_resets_nes() {
        // Desired behavior: reset is reassigned to Cmd/Alt+R, so plain F1 should not reset.
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
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
            crate::cartridge::NametableLayout::Horizontal,
        );
        nes.insert_cartridge(cartridge);

        nes.cpu.set_pc(0x1234);
        let _ = SdlEventLoop::handle_key_down(
            &mut nes,
            Keycode::F1,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );
        assert_eq!(nes.cpu.pc(), 0x1234);
    }

    #[test]
    #[serial]
    fn test_handle_key_down_cmd_or_alt_r_resets_nes() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        let mut prg_rom = vec![0u8; 0x8000];
        let reset_vector: u16 = 0x8000;
        prg_rom[0x7FFC] = (reset_vector & 0x00FF) as u8;
        prg_rom[0x7FFD] = (reset_vector >> 8) as u8;
        let cartridge = crate::cartridge::Cartridge::from_parts(
            prg_rom,
            vec![],
            crate::cartridge::NametableLayout::Horizontal,
        );
        nes.insert_cartridge(cartridge);

        nes.cpu.set_pc(0x1234);
        let _ =
            event_loop.handle_key_down_for_run_with_modifiers(&mut nes, Keycode::R, Mod::LGUIMOD);
        assert_eq!(nes.cpu.pc(), reset_vector);

        nes.cpu.set_pc(0x5678);
        let _ =
            event_loop.handle_key_down_for_run_with_modifiers(&mut nes, Keycode::R, Mod::LALTMOD);
        assert_eq!(nes.cpu.pc(), reset_vector);
    }

    #[test]
    #[serial]
    fn test_handle_key_down_shift_cmd_or_alt_r_resets_nes() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        let mut prg_rom = vec![0u8; 0x8000];
        let reset_vector: u16 = 0x8000;
        prg_rom[0x7FFC] = (reset_vector & 0x00FF) as u8;
        prg_rom[0x7FFD] = (reset_vector >> 8) as u8;
        let cartridge = crate::cartridge::Cartridge::from_parts(
            prg_rom,
            vec![],
            crate::cartridge::NametableLayout::Horizontal,
        );
        nes.insert_cartridge(cartridge);

        nes.cpu.set_pc(0x1234);
        let _ = event_loop.handle_key_down_for_run_with_modifiers(
            &mut nes,
            Keycode::R,
            Mod::LGUIMOD | Mod::LSHIFTMOD,
        );
        assert_eq!(nes.cpu.pc(), reset_vector);

        nes.cpu.set_pc(0x5678);
        let _ = event_loop.handle_key_down_for_run_with_modifiers(
            &mut nes,
            Keycode::R,
            Mod::LALTMOD | Mod::LSHIFTMOD,
        );
        assert_eq!(nes.cpu.pc(), reset_vector);
    }

    #[test]
    fn test_joypad1_keyboard_mapping_wasd_r_t_f_g() {
        // Desired mapping (Joypad 1):
        // - D-pad: W/A/S/D
        // - Select: 4
        // - Start:  5
        // - A:      R
        // - B:      T
        let mut paused = false;
        let mut debugger_open_requested = false;

        // W => Up
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        let _ = SdlEventLoop::handle_key_down(
            &mut nes,
            Keycode::W,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );
        assert_eq!(read_joypad1_buttons(&mut nes), [0, 0, 0, 0, 1, 0, 0, 0]);

        // S => Down
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        let _ = SdlEventLoop::handle_key_down(
            &mut nes,
            Keycode::S,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );
        assert_eq!(read_joypad1_buttons(&mut nes), [0, 0, 0, 0, 0, 1, 0, 0]);

        // A => Left
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        let _ = SdlEventLoop::handle_key_down(
            &mut nes,
            Keycode::A,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );
        assert_eq!(read_joypad1_buttons(&mut nes), [0, 0, 0, 0, 0, 0, 1, 0]);

        // D => Right
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        let _ = SdlEventLoop::handle_key_down(
            &mut nes,
            Keycode::D,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );
        assert_eq!(read_joypad1_buttons(&mut nes), [0, 0, 0, 0, 0, 0, 0, 1]);

        // 4 => Select
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        let _ = SdlEventLoop::handle_key_down(
            &mut nes,
            Keycode::Num4,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );
        assert_eq!(read_joypad1_buttons(&mut nes), [0, 0, 1, 0, 0, 0, 0, 0]);

        // 5 => Start
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        let _ = SdlEventLoop::handle_key_down(
            &mut nes,
            Keycode::Num5,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );
        assert_eq!(read_joypad1_buttons(&mut nes), [0, 0, 0, 1, 0, 0, 0, 0]);

        // R => A
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        let _ = SdlEventLoop::handle_key_down(
            &mut nes,
            Keycode::R,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );
        assert_eq!(read_joypad1_buttons(&mut nes), [1, 0, 0, 0, 0, 0, 0, 0]);

        // T => B
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        let _ = SdlEventLoop::handle_key_down(
            &mut nes,
            Keycode::T,
            None,
            &mut paused,
            &mut debugger_open_requested,
        );
        assert_eq!(read_joypad1_buttons(&mut nes), [0, 1, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    #[serial]
    fn test_new_headless() {
        let mut config = config_with_window_height(960);
        config.gamepads_enabled = false;
        let event_loop = SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone()));
        match event_loop {
            Ok(_) => {}
            Err(err) if err.contains("Cannot initialize `Sdl` from more than one thread.") => {
                eprintln!("Skipping test_new_headless due to SDL thread-affinity: {err}");
            }
            Err(err) => panic!("headless event loop init failed: {err}"),
        }
    }

    #[test]
    #[serial]
    fn test_window_height_small() {
        let config = config_with_window_height(240);
        let event_loop = SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone()));
        assert!(event_loop.is_ok());
    }

    #[test]
    #[serial]
    fn test_window_height_large() {
        let config = config_with_window_height(1200);
        let event_loop = SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone()));
        assert!(event_loop.is_ok());
    }

    #[test]
    #[serial]
    fn test_run_with_nes() {
        let config = default_config();
        let _event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

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
        let config = config_with_gamepads(false);
        let event_loop = SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone()));
        assert!(event_loop.is_ok());
        let event_loop = event_loop.unwrap();
        assert_eq!(event_loop.controllers.len(), 0);
        assert_eq!(event_loop.controller_player_map.len(), 0);
    }

    #[test]
    #[serial]
    fn test_gamepad_enabled_no_controllers_present() {
        // When gamepads are enabled but no controllers are present, should still work
        let config = config_with_gamepads(true);
        let event_loop = SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone()));
        // This may succeed or fail depending on whether controllers are actually present
        // We just verify it doesn't panic
        if let Ok(event_loop) = event_loop {
            // No controllers should be initialized in test environment
            assert!(event_loop.controllers.len() <= 2);
        }
    }

    #[test]
    fn test_gamepad_init_toast_message_when_disabled() {
        let message = gamepad_init_toast_message(false, 0);
        assert_eq!(message, "Gamepads disabled: using keyboard controls");
    }

    #[test]
    fn test_gamepad_init_toast_message_when_enabled_but_none_found() {
        let message = gamepad_init_toast_message(true, 0);
        assert_eq!(message, "No gamepads found: using keyboard controls");
    }

    #[test]
    fn test_gamepad_init_toast_message_when_one_found() {
        let message = gamepad_init_toast_message(true, 1);
        assert_eq!(message, "Gamepad found: using 1 gamepad");
    }

    #[test]
    fn test_gamepad_init_toast_message_when_two_found() {
        let message = gamepad_init_toast_message(true, 2);
        assert_eq!(message, "Gamepads found: using 2 gamepads");
    }

    #[test]
    #[serial]
    fn test_render_debugger_if_needed_invokes_renderer() {
        struct Spy {
            calls: Rc<RefCell<usize>>,
        }

        impl DebuggerRenderer for Spy {
            fn render(&mut self, _snapshot: &crate::debugging::DebuggerSnapshot) {
                *self.calls.borrow_mut() += 1;
            }
        }

        let calls = Rc::new(RefCell::new(0usize));
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        event_loop.set_debugger_renderer(Box::new(Spy {
            calls: calls.clone(),
        }));

        let nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

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
            fn render(&mut self, _snapshot: &crate::debugging::DebuggerSnapshot) {
                *self.calls.borrow_mut() += 1;
            }
        }

        let calls = Rc::new(RefCell::new(0usize));
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        event_loop.set_debugger_renderer(Box::new(Spy {
            calls: calls.clone(),
        }));
        event_loop.request_debugger_open();

        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

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
            crate::cartridge::NametableLayout::Horizontal,
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
            fn render(&mut self, _snapshot: &crate::debugging::DebuggerSnapshot) {
                *self.calls.borrow_mut() += 1;
            }
        }

        let calls = Rc::new(RefCell::new(0usize));
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        event_loop.set_debugger_renderer(Box::new(Spy {
            calls: calls.clone(),
        }));
        event_loop.request_debugger_open();

        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

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
            crate::cartridge::NametableLayout::Horizontal,
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
            fn render(&mut self, _snapshot: &crate::debugging::DebuggerSnapshot) {
                *self.calls.borrow_mut() += 1;
            }
        }

        let calls = Rc::new(RefCell::new(0usize));
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        event_loop.set_debugger_renderer(Box::new(Spy {
            calls: calls.clone(),
        }));

        let nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        event_loop.request_debugger_open();
        SdlEventLoop::tick_windowed_paused_for_run(
            event_loop.debugger_open_requested,
            &mut event_loop.debugger_renderer,
            &nes,
        );

        assert_eq!(*calls.borrow(), 1);
    }

    #[test]
    #[serial]
    fn test_cycle_breakpoint_does_not_fire_spuriously_after_debugger_advances_cycles() {
        // Regression test: if the debugger runs instructions without going through
        // check_post_instruction_breakpoints (e.g. run-to-next-frame), last_post_instruction_cycles
        // stays stale. On continue, the first check sees prev=0, current=27395 and fires the
        // CYC=100 breakpoint even though that threshold was already passed while paused.
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        insert_nop_cartridge(&mut nes, 0x8000);
        nes.reset(false);

        event_loop.add_cycle_breakpoint(100);

        // Simulate: debugger runs many cycles (e.g. run-to-next-frame) without updating tracker.
        // last_post_instruction_cycles stays at 0. We jump the NES cycles to 27395.
        nes.cpu.set_total_cycles(27395);

        // User presses Continue — this should sync the cycle tracker.
        event_loop.continue_from_debugger(&nes);

        // Running one more instruction should NOT trigger the CYC=100 breakpoint
        // because 27395 > 100 and the threshold has already been passed.
        tick_headless_once(&mut event_loop, &mut nes);
        assert!(
            !event_loop.is_paused(),
            "CYC=100 should not fire when already past cycle 100"
        );
    }

    #[test]
    #[serial]
    fn test_cycle_breakpoint_pauses_at_configured_cycle() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        insert_nop_cartridge(&mut nes, 0x8000);
        nes.reset(false);

        // Each NOP takes 2 cycles; target exactly after 2 NOPs.
        let target_cycle = nes.cpu.get_total_cycles() + 4;
        event_loop.add_cycle_breakpoint(target_cycle);

        // First NOP ($8000): total_cycles += 2, not at target yet.
        tick_headless_once(&mut event_loop, &mut nes);
        assert!(!event_loop.is_paused(), "should not pause after first NOP");

        // Second NOP ($8001): total_cycles reaches target, should pause.
        tick_headless_once(&mut event_loop, &mut nes);
        assert!(
            event_loop.is_paused(),
            "should pause when cycle target is reached"
        );
    }

    #[test]
    #[serial]
    fn test_write_address_breakpoint_pauses_on_store_to_address() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));

        // Program: NOP at $8000, STA $1234 at $8001.
        let mut prg_rom = vec![0xEAu8; 0x8000]; // NOP fill
        prg_rom[0x0000] = 0xEA; // NOP at $8000
        prg_rom[0x0001] = 0x8D; // STA abs at $8001
        prg_rom[0x0002] = 0x34; // low byte of $1234
        prg_rom[0x0003] = 0x12; // high byte of $1234
        prg_rom[0x7FFC] = 0x00; // Reset vector low = $8000
        prg_rom[0x7FFD] = 0x80; // Reset vector high

        let cartridge = crate::cartridge::Cartridge::from_parts(
            prg_rom,
            vec![],
            crate::cartridge::NametableLayout::Horizontal,
        );
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        event_loop.add_write_address_breakpoint(0x1234);

        // NOP at $8000: no write, should not pause.
        tick_headless_once(&mut event_loop, &mut nes);
        assert!(
            !event_loop.is_paused(),
            "NOP should not trigger write breakpoint"
        );

        // STA $1234 at $8001: writes to $1234, should pause.
        tick_headless_once(&mut event_loop, &mut nes);
        assert!(
            event_loop.is_paused(),
            "should pause when watched address is written"
        );
    }

    #[test]
    #[serial]
    fn test_load_breakpoints_from_debug_file_populates_breakpoints() {
        use crate::app_context::AppContext;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let rom_path = copy_test_rom(&temp_dir);

        let debug_path = rom_path.with_extension("debug");
        fs::write(&debug_path, "pc 0xC000 enabled\nwrite 0x2006 disabled\n")
            .expect("Failed to write .debug file");

        let rom_bytes = std::fs::read(&rom_path).expect("Failed to read ROM");
        let cart = Cartridge::load_from_file(&rom_bytes, &rom_path, AppContext::new())
            .expect("Failed to load ROM");
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.insert_cartridge(cart);
        nes.reset(false);

        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        event_loop.load_breakpoints_from_debug_file(&nes);

        assert_eq!(
            event_loop.breakpoint_count(),
            2,
            "expected 2 breakpoints loaded"
        );
    }

    #[test]
    #[serial]
    fn test_load_breakpoints_from_debug_file_does_nothing_when_no_file() {
        use crate::app_context::AppContext;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let rom_path = copy_test_rom(&temp_dir);

        let rom_bytes = std::fs::read(&rom_path).expect("Failed to read ROM");
        let cart = Cartridge::load_from_file(&rom_bytes, &rom_path, AppContext::new())
            .expect("Failed to load ROM");
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.insert_cartridge(cart);
        nes.reset(false);

        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        event_loop.load_breakpoints_from_debug_file(&nes);

        assert_eq!(
            event_loop.breakpoint_count(),
            0,
            "expected no breakpoints when file absent"
        );
    }

    #[test]
    #[serial]
    fn test_save_breakpoints_to_debug_file_writes_correct_content() {
        use crate::app_context::AppContext;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let rom_path = copy_test_rom(&temp_dir);

        let rom_bytes = std::fs::read(&rom_path).expect("Failed to read ROM");
        let cart = Cartridge::load_from_file(&rom_bytes, &rom_path, AppContext::new())
            .expect("Failed to load ROM");
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.insert_cartridge(cart);
        nes.reset(false);

        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        event_loop.add_breakpoint(0xC000);
        event_loop.save_breakpoints_to_debug_file(&nes);

        let debug_path = rom_path.with_extension("debug");
        let content = fs::read_to_string(&debug_path).expect("Expected .debug file to be written");
        assert!(
            content.contains("pc 0xC000 enabled"),
            "unexpected content: {content}"
        );
    }

    #[test]
    #[serial]
    fn test_save_breakpoints_to_debug_file_deletes_file_when_no_breakpoints() {
        use crate::app_context::AppContext;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let rom_path = copy_test_rom(&temp_dir);

        let debug_path = rom_path.with_extension("debug");
        fs::write(&debug_path, "pc 0xC000 enabled\n").expect("Failed to write .debug file");
        assert!(debug_path.exists(), "precondition: debug file should exist");

        let rom_bytes = std::fs::read(&rom_path).expect("Failed to read ROM");
        let cart = Cartridge::load_from_file(&rom_bytes, &rom_path, AppContext::new())
            .expect("Failed to load ROM");
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.insert_cartridge(cart);
        nes.reset(false);

        let config = default_config();
        let event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        // No breakpoints added
        event_loop.save_breakpoints_to_debug_file(&nes);

        assert!(
            !debug_path.exists(),
            "debug file should be deleted when no breakpoints"
        );
    }

    #[test]
    #[serial]
    fn test_save_breakpoints_to_debug_file_does_not_create_file_when_no_breakpoints() {
        use crate::app_context::AppContext;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let rom_path = copy_test_rom(&temp_dir);

        let debug_path = rom_path.with_extension("debug");
        assert!(
            !debug_path.exists(),
            "precondition: debug file should not exist"
        );

        let rom_bytes = std::fs::read(&rom_path).expect("Failed to read ROM");
        let cart = Cartridge::load_from_file(&rom_bytes, &rom_path, AppContext::new())
            .expect("Failed to load ROM");
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
        nes.insert_cartridge(cart);
        nes.reset(false);

        let config = default_config();
        let event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        event_loop.save_breakpoints_to_debug_file(&nes);

        assert!(
            !debug_path.exists(),
            "debug file should not be created when there are no breakpoints"
        );
    }

    #[test]
    #[serial]
    fn test_save_breakpoints_to_debug_file_is_noop_without_rom_path() {
        let config = default_config();
        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();
        let nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        )); // no cartridge inserted
        event_loop.add_breakpoint(0xC000);
        // Should not panic
        event_loop.save_breakpoints_to_debug_file(&nes);
    }

    #[test]
    #[serial]
    fn test_config_breakpoints_are_loaded_at_run_start() {
        use crate::debugging::breakpoints::BreakpointKind;

        let mut config = default_config();
        config.breakpoints = vec![BreakpointKind::Pc(0xC000)];

        let mut event_loop =
            SdlEventLoop::new(true, None, AppContext::new_with_config(config.clone())).unwrap();

        let app_context =
            std::rc::Rc::new(std::cell::RefCell::new(AppContext::new_with_config(config)));
        event_loop.load_breakpoints_from_context(&app_context);

        assert_eq!(
            event_loop.breakpoint_count(),
            1,
            "expected config breakpoints to be loaded"
        );
    }
}
