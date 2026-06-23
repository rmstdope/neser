//! Keyboard input and hotkey handling for the native frontend.
//!
//! Exposes the entry points ([`handle_key_pressed`], [`handle_key_released`],
//! [`keyboard_target_ports`], [`KeyOutcome`]) and delegates to per-domain
//! submodules: [`hotkeys`] (system/debugger/cartridge-switch hotkeys),
//! [`console_keyboard`] (per-console press dispatch), and [`controller_mapping`]
//! (key→button mapping tables).

use crate::frontends::native::app_state::NativeAppState;
use crate::platform::audio::EmulatorAudio;
use crate::platform::emulator::Console;
use winit::keyboard::KeyCode;

mod console_keyboard;
mod controller_mapping;
mod hotkeys;

/// The result of processing a key-press event.
#[derive(Debug, PartialEq, Eq)]
pub enum KeyOutcome {
    /// The user requested the application to exit (e.g. Ctrl+Q).
    Quit,
    /// Normal processing; the event loop should continue.
    Continue,
    /// The shader preset should be cycled (F4).
    CycleShader,
    /// Toggle the debugger open/closed (F5).
    ToggleDebugger,
    /// Step over the current instruction (F10).
    StepOver,
    /// Step into the current instruction (F11).
    StepInto,
    /// The user selected a cartridge to load (Enter in cart-switch dialog).
    SwitchCartridge(String),
    /// The user requested the cartridge-switch dialog (Ctrl+O).
    OpenCartridgeSwitch,
    /// The user closed the cartridge-switch dialog (Escape).
    CloseCartridgeSwitch,
    /// Toggle the FPS counter overlay (F1).
    ToggleFps,
    /// Cycle to the next preset NES system palette (F8).
    CyclePalette,
}

/// Handles a key-press event.
///
/// Updates `console` and `app_state` as appropriate, and optionally adjusts
/// audio volume via `audio`.  Returns a [`KeyOutcome`] so the caller can act
/// on actions that require access to the GL wrapper (e.g. shader cycling,
/// fullscreen toggle).
///
/// For [`Console::GameBoy`], NES-specific features such as SNES buttons and
/// Power Pad are ignored, but generic features including the debugger work for
/// both systems.
pub fn handle_key_pressed(
    console: &mut Console,
    key_code: KeyCode,
    app_state: &mut NativeAppState,
    audio: Option<&dyn EmulatorAudio>,
) -> KeyOutcome {
    match console {
        Console::Nes(_) => {
            if app_state.cart_switch.open {
                return hotkeys::handle_cartridge_switch_key(key_code, app_state);
            }
            if app_state.modifiers.control_key() {
                return hotkeys::handle_ctrl_hotkey(console, key_code, app_state);
            }
            console_keyboard::handle_unmodified_key(console, key_code, app_state, audio)
        }
        Console::GameBoy(_) => {
            console_keyboard::handle_gameboy_key_pressed(console, key_code, app_state, audio)
        }
        Console::GameBoyAdvance(_) => {
            console_keyboard::handle_gba_key_pressed(console, key_code, app_state, audio)
        }
        Console::Snes(_) => {
            console_keyboard::handle_snes_key_pressed(console, key_code, app_state, audio)
        }
    }
}

/// Handles a key-release event.
///
/// Releases the NES / SNES / Power Pad button corresponding to the given key.
/// `gamepad_count` and `four_score` determine which ports keyboard input is
/// routed to.
///
/// For [`Console::GameBoy`], only generic directional/A/B/Start/Select
/// releases are dispatched; NES-specific keys are ignored.
pub fn handle_key_released(
    console: &mut Console,
    key_code: KeyCode,
    gamepad_count: usize,
    four_score: bool,
) {
    match console {
        Console::Nes(_) => {
            let ports = keyboard_target_ports(gamepad_count, four_score);
            controller_mapping::handle_controller_key(console, key_code, false, ports);
        }
        Console::GameBoy(_) => {
            if let Some(btn_id) = controller_mapping::gameboy_key_to_button_id(key_code) {
                console.set_button(0, btn_id, false);
            }
        }
        Console::GameBoyAdvance(_) => {
            if let Some(btn_id) = controller_mapping::gba_key_to_button_id(key_code) {
                console.set_button(0, btn_id, false);
            }
        }
        Console::Snes(_) => {
            if let Some(btn_id) = controller_mapping::snes_key_to_button_id(key_code) {
                console.set_button(0, btn_id, false);
            }
        }
    }
}

/// Returns the NES ports that keyboard input should be routed to, based on
/// how many gamepads are connected and whether Four Score mode is active.
///
/// Without Four Score (max 2 ports):
/// - 0 gamepads → `[1, 2]` (keyboard covers both ports)
/// - 1 gamepad  → `[2]`    (gamepad takes port 1; keyboard takes port 2)
/// - 2+ gamepads → `[]`    (both ports owned by gamepads; keyboard disabled)
///
/// With Four Score (max 4 ports):
/// - 0 gamepads → `[1, 2]`
/// - 1 gamepad  → `[2, 3]`
/// - 2 gamepads → `[3, 4]`
/// - 3 gamepads → `[4]`
/// - 4+ gamepads → `[]`
pub fn keyboard_target_ports(gamepad_count: usize, four_score: bool) -> &'static [u8] {
    if four_score {
        match gamepad_count {
            0 => &[1, 2],
            1 => &[2, 3],
            2 => &[3, 4],
            3 => &[4],
            _ => &[],
        }
    } else {
        match gamepad_count {
            0 => &[1, 2],
            1 => &[2],
            _ => &[],
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::console::{Config, Nes, NesConfig};
    use crate::nes::input::{Button, PowerPadButton, SnesButton};
    use crate::platform::app_context::AppContext;
    use crate::platform::emulator::Console;
    use winit::keyboard::ModifiersState;

    // ── Test helpers ──────────────────────────────────────────────────────────

    #[test]
    fn snes_key_mapping_covers_face_and_shoulder_buttons() {
        // Base GB-style keys still map.
        assert_eq!(
            controller_mapping::snes_key_to_button_id(KeyCode::KeyT),
            Some(0)
        ); // A
        assert_eq!(
            controller_mapping::snes_key_to_button_id(KeyCode::KeyR),
            Some(1)
        ); // B
        assert_eq!(
            controller_mapping::snes_key_to_button_id(KeyCode::Digit4),
            Some(2)
        ); // Select
        assert_eq!(
            controller_mapping::snes_key_to_button_id(KeyCode::Digit5),
            Some(3)
        ); // Start
        // SNES additions.
        assert_eq!(
            controller_mapping::snes_key_to_button_id(KeyCode::KeyQ),
            Some(8)
        ); // L
        assert_eq!(
            controller_mapping::snes_key_to_button_id(KeyCode::KeyE),
            Some(9)
        ); // R
        assert_eq!(
            controller_mapping::snes_key_to_button_id(KeyCode::KeyY),
            Some(10)
        ); // X
        assert_eq!(
            controller_mapping::snes_key_to_button_id(KeyCode::KeyG),
            Some(11)
        ); // Y
        assert_eq!(controller_mapping::snes_key_to_button_id(KeyCode::F1), None);
    }

    fn make_nes() -> Nes {
        Nes::new(AppContext::new_with_config(Config::default()))
    }

    fn make_nes_four_score() -> Nes {
        Nes::new(AppContext::new_with_config(Config {
            nes: NesConfig {
                four_score_enabled: true,
                ..Default::default()
            },
            ..Config::default()
        }))
    }

    fn make_nes_with_cartridge() -> Nes {
        let mut nes = make_nes();
        let mut prg_rom = vec![0xEAu8; 0x8000]; // NOP
        prg_rom[0x7FFC] = 0x00;
        prg_rom[0x7FFD] = 0x80;
        prg_rom[0x7FFA] = 0x00;
        prg_rom[0x7FFB] = 0x80;
        prg_rom[0x7FFE] = 0x00;
        prg_rom[0x7FFF] = 0x80;
        let cartridge = crate::nes::cartridge::Cartridge::from_parts(
            prg_rom,
            vec![],
            crate::nes::cartridge::NametableLayout::Horizontal,
        );
        nes.insert_cartridge(cartridge);
        nes
    }

    fn make_nes_console() -> Console {
        Console::Nes(Box::new(make_nes()))
    }

    fn make_nes_console_with_cart() -> Console {
        Console::Nes(Box::new(make_nes_with_cartridge()))
    }

    fn make_nes_console_four_score() -> Console {
        Console::Nes(Box::new(make_nes_four_score()))
    }

    fn make_gba_console() -> Console {
        Console::new_gba(AppContext::new_with_config(Config::default()))
    }

    fn minimal_snes_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x10000];
        let header = 0x7FC0;
        rom[header..header + 21].copy_from_slice(b"SNES TEST ROM        ");
        rom[header + 0x3C] = 0x00;
        rom[header + 0x3D] = 0x80;
        rom[header + 0xD5] = 0x20;
        rom[header + 0xD6] = 0x00;
        rom[header + 0xD7] = 0x07;
        rom[header + 0xD8] = 0x00;
        rom[header + 0xD9] = 0x00;
        rom[header + 0xDC] = 0x34;
        rom[header + 0xDD] = 0x12;
        rom[header + 0xDE] = 0xCB;
        rom[header + 0xDF] = 0xED;
        rom[0x0000] = 0xEA;
        rom
    }

    fn make_snes_console(rom_name: &str) -> Console {
        let mut console = Console::new_snes(AppContext::new_with_config(Config::default()));
        console
            .load_rom(&minimal_snes_rom(), rom_name)
            .expect("minimal SNES ROM should load");
        console
    }

    fn gba_keyinput(console: &Console) -> u16 {
        let Console::GameBoyAdvance(gba) = console else {
            panic!("expected GBA console");
        };
        gba.bus().keypad.read_keyinput()
    }

    fn make_state() -> NativeAppState {
        NativeAppState::default()
    }

    fn with_ctrl(state: &mut NativeAppState) {
        state.modifiers = ModifiersState::CONTROL;
    }

    fn with_ctrl_shift(state: &mut NativeAppState) {
        state.modifiers = ModifiersState::CONTROL | ModifiersState::SHIFT;
    }

    #[allow(dead_code)]
    fn buttons(nes: &Nes, port: u8) -> u8 {
        nes.get_joypad_button_states(port)
    }

    const BIT_A: u8 = 1 << Button::A as u8;
    const BIT_B: u8 = 1 << Button::B as u8;
    const BIT_SELECT: u8 = 1 << Button::Select as u8;
    const BIT_START: u8 = 1 << Button::Start as u8;
    const BIT_UP: u8 = 1 << Button::Up as u8;
    const BIT_DOWN: u8 = 1 << Button::Down as u8;
    const BIT_LEFT: u8 = 1 << Button::Left as u8;
    const BIT_RIGHT: u8 = 1 << Button::Right as u8;

    const GBA_KEY_A: u16 = 1 << 0;
    const GBA_KEY_B: u16 = 1 << 1;
    const GBA_KEY_SELECT: u16 = 1 << 2;
    const GBA_KEY_START: u16 = 1 << 3;
    const GBA_KEY_RIGHT: u16 = 1 << 4;
    const GBA_KEY_LEFT: u16 = 1 << 5;
    const GBA_KEY_UP: u16 = 1 << 6;
    const GBA_KEY_DOWN: u16 = 1 << 7;
    const GBA_KEY_R: u16 = 1 << 8;
    const GBA_KEY_L: u16 = 1 << 9;

    // ── Mock audio ────────────────────────────────────────────────────────────

    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    struct MockAudio {
        volume: Arc<AtomicU32>,
    }

    /// Mock audio that tracks whether pause(), resume(), and drain_buffer() have been called.
    struct TrackingMockAudio {
        pause_called: Arc<AtomicBool>,
        resume_called: Arc<AtomicBool>,
        drain_buffer_called: Arc<AtomicBool>,
    }

    impl TrackingMockAudio {
        fn new() -> (Self, Arc<AtomicBool>, Arc<AtomicBool>, Arc<AtomicBool>) {
            let pause_called = Arc::new(AtomicBool::new(false));
            let resume_called = Arc::new(AtomicBool::new(false));
            let drain_buffer_called = Arc::new(AtomicBool::new(false));
            let audio = Self {
                pause_called: Arc::clone(&pause_called),
                resume_called: Arc::clone(&resume_called),
                drain_buffer_called: Arc::clone(&drain_buffer_called),
            };
            (audio, pause_called, resume_called, drain_buffer_called)
        }
    }

    impl EmulatorAudio for TrackingMockAudio {
        fn queue_sample(&mut self, _sample: f32) {}
        fn resume(&self) {
            self.resume_called.store(true, Ordering::Relaxed);
        }
        fn pause(&self) {
            self.pause_called.store(true, Ordering::Relaxed);
        }
        fn drain_buffer(&self) {
            self.drain_buffer_called.store(true, Ordering::Relaxed);
        }
        fn set_volume(&self, _volume: f32) {}
        fn get_volume(&self) -> f32 {
            0.0
        }
        fn prime_startup(&mut self, _samples: usize) {}
        fn take_and_reset_stats(&self) -> (u64, u64, u64) {
            (0, 0, 0)
        }
        fn actual_sample_rate(&self) -> i32 {
            44100
        }
    }

    impl MockAudio {
        fn new_with_volume(vol: f32) -> Self {
            Self {
                volume: Arc::new(AtomicU32::new(f32::to_bits(vol))),
            }
        }
    }

    impl EmulatorAudio for MockAudio {
        fn queue_sample(&mut self, _sample: f32) {}
        fn resume(&self) {}
        fn pause(&self) {}
        fn set_volume(&self, volume: f32) {
            self.volume
                .store(f32::to_bits(volume.clamp(0.0, 1.0)), Ordering::Relaxed);
        }
        fn get_volume(&self) -> f32 {
            f32::from_bits(self.volume.load(Ordering::Relaxed))
        }
        fn prime_startup(&mut self, _samples: usize) {}
        fn take_and_reset_stats(&self) -> (u64, u64, u64) {
            (0, 0, 0)
        }
        fn actual_sample_rate(&self) -> i32 {
            44100
        }
    }

    // ── Hotkey tests ──────────────────────────────────────────────────────────

    #[test]
    fn test_ctrl_q_returns_quit() {
        let mut console = make_nes_console();
        let mut state = make_state();
        with_ctrl(&mut state);
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::KeyQ, &mut state, None),
            KeyOutcome::Quit
        );
    }

    #[test]
    fn test_escape_returns_continue_when_mouse_not_grabbed() {
        let mut console = make_nes_console();
        let mut state = make_state();
        state.mouse_grabbed = false;
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::Escape, &mut state, None),
            KeyOutcome::Continue
        );
    }

    #[test]
    fn test_escape_releases_mouse_when_grabbed() {
        let mut console = make_nes_console();
        let mut state = make_state();
        state.mouse_grabbed = true;
        handle_key_pressed(&mut console, KeyCode::Escape, &mut state, None);
        assert!(!state.mouse_grabbed, "Escape should release mouse grab");
        assert!(
            state.mouse_released_by_escape,
            "Escape should set mouse_released_by_escape"
        );
    }

    #[test]
    fn test_space_toggles_pause_on() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::Space, &mut state, None);
        assert!(state.paused, "Space should pause when unpaused");
    }

    #[test]
    fn test_space_toggles_pause_off() {
        let mut console = make_nes_console();
        let mut state = make_state();
        state.paused = true;
        handle_key_pressed(&mut console, KeyCode::Space, &mut state, None);
        assert!(!state.paused, "Space should unpause when paused");
    }

    #[test]
    fn test_space_calls_audio_pause_when_pausing() {
        let mut console = make_nes_console();
        let mut state = make_state();
        state.paused = false;
        let (audio, pause_called, _resume_called, _drain_buffer_called) = TrackingMockAudio::new();
        handle_key_pressed(
            &mut console,
            KeyCode::Space,
            &mut state,
            Some(&audio as &dyn EmulatorAudio),
        );
        assert!(
            pause_called.load(Ordering::Relaxed),
            "Space (pause) should call audio.pause() to prevent ring buffer drain"
        );
    }

    #[test]
    fn test_space_calls_audio_resume_when_resuming() {
        let mut console = make_nes_console();
        let mut state = make_state();
        state.paused = true;
        let (audio, _pause_called, resume_called, _drain_buffer_called) = TrackingMockAudio::new();
        handle_key_pressed(
            &mut console,
            KeyCode::Space,
            &mut state,
            Some(&audio as &dyn EmulatorAudio),
        );
        assert!(
            resume_called.load(Ordering::Relaxed),
            "Space (resume) should call audio.resume() to restore audio after pause"
        );
    }

    #[test]
    fn test_h_toggles_help_overlay_on() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyH, &mut state, None);
        assert!(state.help_overlay_visible);
    }

    #[test]
    fn test_h_toggles_help_overlay_off() {
        let mut console = make_nes_console();
        let mut state = make_state();
        state.help_overlay_visible = true;
        handle_key_pressed(&mut console, KeyCode::KeyH, &mut state, None);
        assert!(!state.help_overlay_visible);
    }

    #[test]
    fn test_ctrl_f_toggles_fullscreen_on() {
        let mut console = make_nes_console();
        let mut state = make_state();
        with_ctrl(&mut state);
        handle_key_pressed(&mut console, KeyCode::KeyF, &mut state, None);
        assert!(state.fullscreen);
    }

    #[test]
    fn test_ctrl_f_toggles_fullscreen_off() {
        let mut console = make_nes_console();
        let mut state = make_state();
        state.fullscreen = true;
        with_ctrl(&mut state);
        handle_key_pressed(&mut console, KeyCode::KeyF, &mut state, None);
        assert!(!state.fullscreen);
    }

    #[test]
    fn test_alt_f_does_not_toggle_fullscreen() {
        let mut console = make_nes_console();
        let mut state = make_state();
        state.modifiers = ModifiersState::ALT;
        handle_key_pressed(&mut console, KeyCode::KeyF, &mut state, None);
        assert!(!state.fullscreen, "Alt+F should not toggle fullscreen");
    }

    #[test]
    fn test_f4_returns_cycle_shader() {
        let mut console = make_nes_console();
        let mut state = make_state();
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::F4, &mut state, None),
            KeyOutcome::CycleShader
        );
    }

    #[test]
    fn test_f2_increases_volume() {
        let mut console = make_nes_console();
        let mut state = make_state();
        let audio = MockAudio::new_with_volume(0.5);
        handle_key_pressed(&mut console, KeyCode::F2, &mut state, Some(&audio));
        let vol = audio.get_volume();
        assert!(
            (vol - 0.6).abs() < 1e-5,
            "F2 should raise volume by 0.1 (got {vol})"
        );
    }

    #[test]
    fn test_f3_decreases_volume() {
        let mut console = make_nes_console();
        let mut state = make_state();
        let audio = MockAudio::new_with_volume(0.5);
        handle_key_pressed(&mut console, KeyCode::F3, &mut state, Some(&audio));
        let vol = audio.get_volume();
        assert!(
            (vol - 0.4).abs() < 1e-5,
            "F3 should lower volume by 0.1 (got {vol})"
        );
    }

    #[test]
    fn test_f5_returns_toggle_debugger() {
        let mut console = make_nes_console();
        let mut state = make_state();
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::F5, &mut state, None),
            KeyOutcome::ToggleDebugger
        );
    }

    #[test]
    fn test_f8_returns_cycle_palette_for_nes() {
        let mut console = make_nes_console();
        let mut state = make_state();
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::F8, &mut state, None),
            KeyOutcome::CyclePalette
        );
    }

    #[test]
    fn test_f10_returns_step_over() {
        let mut console = make_nes_console();
        let mut state = make_state();
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::F10, &mut state, None),
            KeyOutcome::StepOver
        );
    }

    #[test]
    fn test_f11_returns_step_into() {
        let mut console = make_nes_console();
        let mut state = make_state();
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::F11, &mut state, None),
            KeyOutcome::StepInto
        );
    }

    #[test]
    fn test_ctrl_r_soft_resets() {
        let mut console = make_nes_console_with_cart();
        let mut state = make_state();
        with_ctrl(&mut state);
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::KeyR, &mut state, None),
            KeyOutcome::Continue
        );
    }

    #[test]
    fn test_ctrl_shift_r_hard_resets() {
        let mut console = make_nes_console_with_cart();
        let mut state = make_state();
        with_ctrl_shift(&mut state);
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::KeyR, &mut state, None),
            KeyOutcome::Continue
        );
    }

    // ── Player 1 standard button mapping ──────────────────────────────────────

    #[test]
    fn test_p1_w_sets_up() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyW, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(1) & BIT_UP,
            0,
            "W should set Up on P1"
        );
    }

    #[test]
    fn test_p1_a_sets_left() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyA, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(1) & BIT_LEFT,
            0,
            "A should set Left on P1"
        );
    }

    #[test]
    fn test_p1_s_sets_down() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyS, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(1) & BIT_DOWN,
            0,
            "S should set Down on P1"
        );
    }

    #[test]
    fn test_p1_d_sets_right() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyD, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(1) & BIT_RIGHT,
            0,
            "D should set Right on P1"
        );
    }

    #[test]
    fn test_p1_t_sets_a() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyT, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(1) & BIT_A,
            0,
            "T should set A on P1"
        );
    }

    #[test]
    fn test_p1_r_sets_b() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyR, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(1) & BIT_B,
            0,
            "R should set B on P1"
        );
    }

    #[test]
    fn test_p1_num4_sets_select() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::Digit4, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(1) & BIT_SELECT,
            0,
            "4 should set Select on P1"
        );
    }

    #[test]
    fn test_p1_num5_sets_start() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::Digit5, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(1) & BIT_START,
            0,
            "5 should set Start on P1"
        );
    }

    // ── Player 1 — key release clears button ──────────────────────────────────

    #[test]
    fn test_p1_w_released_clears_up() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyW, &mut state, None);
        assert_ne!(console.get_joypad_button_states(1) & BIT_UP, 0);
        handle_key_released(&mut console, KeyCode::KeyW, 0, false);
        assert_eq!(
            console.get_joypad_button_states(1) & BIT_UP,
            0,
            "Releasing W should clear Up"
        );
    }

    #[test]
    fn test_p1_t_released_clears_a() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyT, &mut state, None);
        handle_key_released(&mut console, KeyCode::KeyT, 0, false);
        assert_eq!(
            console.get_joypad_button_states(1) & BIT_A,
            0,
            "Releasing T should clear A"
        );
    }

    // ── Player 2 standard button mapping ──────────────────────────────────────

    #[test]
    fn test_p2_i_sets_up() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyI, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(2) & BIT_UP,
            0,
            "I should set Up on P2"
        );
        assert_eq!(
            console.get_joypad_button_states(1) & BIT_UP,
            0,
            "I should not affect P1"
        );
    }

    #[test]
    fn test_p2_j_sets_left() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyJ, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(2) & BIT_LEFT,
            0,
            "J should set Left on P2"
        );
    }

    #[test]
    fn test_p2_k_sets_down() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyK, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(2) & BIT_DOWN,
            0,
            "K should set Down on P2"
        );
    }

    #[test]
    fn test_p2_l_sets_right() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyL, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(2) & BIT_RIGHT,
            0,
            "L should set Right on P2"
        );
    }

    #[test]
    fn test_p2_o_sets_a() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyO, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(2) & BIT_A,
            0,
            "O should set A on P2"
        );
    }

    #[test]
    fn test_p2_p_sets_b() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyP, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(2) & BIT_B,
            0,
            "P should set B on P2"
        );
    }

    #[test]
    fn test_p2_num9_sets_select() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::Digit9, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(2) & BIT_SELECT,
            0,
            "9 should set Select on P2"
        );
    }

    #[test]
    fn test_p2_num0_sets_start() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::Digit0, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(2) & BIT_START,
            0,
            "0 should set Start on P2"
        );
    }

    // ── P1 keys target port 1 only (not port 2) when no gamepad ─────────────

    #[test]
    fn test_w_targets_port1_only_when_no_gamepad() {
        let mut console = make_nes_console();
        let mut state = make_state(); // gamepad_count = 0
        handle_key_pressed(&mut console, KeyCode::KeyW, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(1) & BIT_UP,
            0,
            "W should set Up on P1"
        );
        assert_eq!(
            console.get_joypad_button_states(2) & BIT_UP,
            0,
            "W should NOT set Up on P2 (port 2 has dedicated IJKL keys)"
        );
    }

    #[test]
    fn test_s_targets_port1_only_when_no_gamepad() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyS, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(1) & BIT_DOWN,
            0,
            "S should set Down on P1"
        );
        assert_eq!(
            console.get_joypad_button_states(2) & BIT_DOWN,
            0,
            "S should NOT set Down on P2"
        );
    }

    // ── Player 2 key release ──────────────────────────────────────────────────

    #[test]
    fn test_p2_i_released_clears_up() {
        let mut console = make_nes_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyI, &mut state, None);
        handle_key_released(&mut console, KeyCode::KeyI, 0, false);
        assert_eq!(
            console.get_joypad_button_states(2) & BIT_UP,
            0,
            "Releasing I should clear Up on P2"
        );
    }

    // ── Cartridge-switch dialog keyboard ──────────────────────────────────────

    fn make_cart_switch_state(entries: &[&str]) -> NativeAppState {
        let mut state = make_state();
        state.cart_switch.open = true;
        state.cart_switch.entries = entries.iter().map(|s| s.to_string()).collect();
        state
    }

    #[test]
    fn test_cart_switch_typing_adds_to_filter() {
        let mut console = make_nes_console();
        let mut state = make_cart_switch_state(&["alpha.nes", "beta.nes"]);
        handle_key_pressed(&mut console, KeyCode::KeyA, &mut state, None);
        assert_eq!(state.cart_switch.filter, "a");
        handle_key_pressed(&mut console, KeyCode::KeyB, &mut state, None);
        assert_eq!(state.cart_switch.filter, "ab");
    }

    #[test]
    fn test_cart_switch_backspace_removes_char() {
        let mut console = make_nes_console();
        let mut state = make_cart_switch_state(&["a.nes"]);
        state.cart_switch.filter = "abc".to_string();
        let outcome = handle_key_pressed(&mut console, KeyCode::Backspace, &mut state, None);
        assert_eq!(state.cart_switch.filter, "ab");
        assert_eq!(outcome, KeyOutcome::Continue);
    }

    #[test]
    fn test_cart_switch_backspace_on_empty_filter() {
        let mut console = make_nes_console();
        let mut state = make_cart_switch_state(&["a.nes"]);
        let outcome = handle_key_pressed(&mut console, KeyCode::Backspace, &mut state, None);
        assert!(state.cart_switch.filter.is_empty());
        assert_eq!(outcome, KeyOutcome::Continue);
    }

    #[test]
    fn test_cart_switch_enter_returns_switch_cartridge() {
        let mut console = make_nes_console();
        let mut state = make_cart_switch_state(&["game.nes", "other.nes"]);
        state.cart_switch.selection = 0;
        let outcome = handle_key_pressed(&mut console, KeyCode::Enter, &mut state, None);
        assert_eq!(outcome, KeyOutcome::SwitchCartridge("game.nes".to_string()));
        assert!(!state.cart_switch.open, "dialog should close after Enter");
    }

    #[test]
    fn test_cart_switch_enter_with_no_entries_returns_continue() {
        let mut console = make_nes_console();
        let mut state = make_cart_switch_state(&[]);
        let outcome = handle_key_pressed(&mut console, KeyCode::Enter, &mut state, None);
        assert_eq!(outcome, KeyOutcome::Continue);
    }

    #[test]
    fn test_cart_switch_escape_closes_dialog() {
        let mut console = make_nes_console();
        let mut state = make_cart_switch_state(&["a.nes"]);
        handle_key_pressed(&mut console, KeyCode::Escape, &mut state, None);
        assert!(!state.cart_switch.open);
    }

    #[test]
    fn test_cart_switch_arrow_down_wraps() {
        let mut console = make_nes_console();
        let mut state = make_cart_switch_state(&["a.nes", "b.nes"]);
        state.cart_switch.selection = 1;
        handle_key_pressed(&mut console, KeyCode::ArrowDown, &mut state, None);
        assert_eq!(state.cart_switch.selection, 0, "should wrap to first");
    }

    #[test]
    fn test_cart_switch_arrow_up_wraps() {
        let mut console = make_nes_console();
        let mut state = make_cart_switch_state(&["a.nes", "b.nes"]);
        state.cart_switch.selection = 0;
        handle_key_pressed(&mut console, KeyCode::ArrowUp, &mut state, None);
        assert_eq!(state.cart_switch.selection, 1, "should wrap to last");
    }

    #[test]
    fn test_cart_switch_typing_filters_entries() {
        let mut console = make_nes_console();
        let mut state = make_cart_switch_state(&["alpha.nes", "beta.nes", "gamma.nes"]);
        // Type 'b' — only "beta.nes" matches (fuzzy on filename)
        handle_key_pressed(&mut console, KeyCode::KeyB, &mut state, None);
        assert_eq!(state.cart_switch.visible_count(), 1);
        assert_eq!(state.cart_switch.selected_entry(), Some("beta.nes"));
    }

    #[test]
    fn test_cart_switch_keys_dont_trigger_controller() {
        let mut console = make_nes_console_with_cart();
        let mut state = make_cart_switch_state(&["a.nes"]);
        // Pressing W while cart switch is open should NOT set Up on P1
        handle_key_pressed(&mut console, KeyCode::KeyW, &mut state, None);
        assert_eq!(
            console.get_joypad_button_states(1) & BIT_UP,
            0,
            "W should not control NES when dialog is open"
        );
    }

    // ── Review fix: Escape returns CloseCartridgeSwitch ───────────────────────

    #[test]
    fn test_cart_switch_escape_returns_close_outcome() {
        let mut console = make_nes_console();
        let mut state = make_cart_switch_state(&["a.nes"]);
        let outcome = handle_key_pressed(&mut console, KeyCode::Escape, &mut state, None);
        assert_eq!(
            outcome,
            KeyOutcome::CloseCartridgeSwitch,
            "Escape should return CloseCartridgeSwitch so event loop can restore pause"
        );
    }

    // ── Review fix: underscore via Shift+Minus ────────────────────────────────

    #[test]
    fn test_cart_switch_shift_minus_types_underscore() {
        let mut console = make_nes_console();
        let mut state = make_cart_switch_state(&["a_b.nes"]);
        state.modifiers = ModifiersState::SHIFT;
        handle_key_pressed(&mut console, KeyCode::Minus, &mut state, None);
        assert_eq!(
            state.cart_switch.filter, "_",
            "Shift+Minus should type underscore"
        );
    }

    #[test]
    fn test_cart_switch_minus_without_shift_types_dash() {
        let mut console = make_nes_console();
        let mut state = make_cart_switch_state(&["a-b.nes"]);
        handle_key_pressed(&mut console, KeyCode::Minus, &mut state, None);
        assert_eq!(
            state.cart_switch.filter, "-",
            "Minus without Shift should type dash"
        );
    }

    #[test]
    fn test_f7_drains_audio_buffer_after_state_load() {
        // When F7 (load state) is pressed, any pre-restore samples still buffered
        // in the audio ring buffer must be discarded silently so they do not
        // bleed into the post-restore playback.
        let mut console = make_nes_console();
        let mut state = make_state();
        let (audio, _pause_called, _resume_called, drain_buffer_called) = TrackingMockAudio::new();
        handle_key_pressed(
            &mut console,
            KeyCode::F7,
            &mut state,
            Some(&audio as &dyn EmulatorAudio),
        );
        assert!(
            drain_buffer_called.load(Ordering::Relaxed),
            "F7 (load state) must call audio.drain_buffer() to discard stale samples"
        );
    }

    // ── Gamepad-count-aware keyboard routing ──────────────────────────────────

    #[test]
    fn test_wasd_routes_to_port2_only_when_one_gamepad() {
        // Given: one gamepad connected (port 1 owned by gamepad)
        let mut console = make_nes_console();
        let mut state = NativeAppState {
            gamepad_count: 1,
            ..NativeAppState::default()
        };
        // When: W (Up) is pressed
        handle_key_pressed(&mut console, KeyCode::KeyW, &mut state, None);
        // Then: port 2 gets Up; port 1 does NOT (gamepad owns port 1)
        assert_ne!(
            console.get_joypad_button_states(2) & BIT_UP,
            0,
            "W should set port 2 Up when one gamepad is connected"
        );
        assert_eq!(
            console.get_joypad_button_states(1) & BIT_UP,
            0,
            "W should NOT set port 1 Up when one gamepad is connected"
        );
    }

    #[test]
    fn test_wasd_disabled_when_two_gamepads() {
        // Given: two gamepads connected (both ports owned by gamepads)
        let mut console = make_nes_console();
        let mut state = NativeAppState {
            gamepad_count: 2,
            ..NativeAppState::default()
        };
        // When: W (Up) is pressed
        handle_key_pressed(&mut console, KeyCode::KeyW, &mut state, None);
        // Then: neither port gets input
        assert_eq!(
            console.get_joypad_button_states(1) & BIT_UP,
            0,
            "W should NOT set port 1 Up when two gamepads are connected"
        );
        assert_eq!(
            console.get_joypad_button_states(2) & BIT_UP,
            0,
            "W should NOT set port 2 Up when two gamepads are connected"
        );
    }

    #[test]
    fn test_ijkl_disabled_when_two_gamepads() {
        // Given: two gamepads connected
        let mut console = make_nes_console();
        let mut state = NativeAppState {
            gamepad_count: 2,
            ..NativeAppState::default()
        };
        // When: I (P2 Up) is pressed
        handle_key_pressed(&mut console, KeyCode::KeyI, &mut state, None);
        // Then: port 2 gets no input
        assert_eq!(
            console.get_joypad_button_states(2) & BIT_UP,
            0,
            "I (P2 Up) should be disabled when two gamepads are connected"
        );
    }

    #[test]
    fn test_ijkl_disabled_when_one_gamepad() {
        // With 1 gamepad, port 1 is owned by the gamepad.  The keyboard player
        // on port 2 should use WASD (the P1 key set, which shifts to track
        // ports.first()).  The P2-specific IJKL keys should be disabled because
        // there is no dedicated keyboard "player 2" slot.
        let mut console = make_nes_console();
        let mut state = NativeAppState {
            gamepad_count: 1,
            ..NativeAppState::default()
        };
        handle_key_pressed(&mut console, KeyCode::KeyI, &mut state, None);
        assert_eq!(
            console.get_joypad_button_states(2) & BIT_UP,
            0,
            "I (P2 Up) should be disabled when one gamepad is connected; use WASD instead"
        );
    }

    #[test]
    fn test_help_overlay_port2_shows_wasd_not_ijkl_when_one_gamepad() {
        // When 1 gamepad is connected the keyboard player is on port 2 using
        // the WASD key set (P1 keys shift to ports.first() = port 2).
        // The IJKL keys do nothing, so the help text must NOT list them for
        // port 2 and MUST list WASD for port 2.
        let state = crate::frontends::native::app_state::NativeAppState {
            help_overlay_visible: true,
            gamepad_count: 1,
            ..Default::default()
        };
        let nes = make_nes_console();
        let text = state
            .overlay_text(&nes, None)
            .expect("help overlay must be present");
        assert!(
            text.contains("W/A/S/D"),
            "help overlay must list W/A/S/D for port 2 with 1 gamepad; got:\n{text}"
        );
        assert!(
            !text.contains("I/J/K/L"),
            "help overlay must NOT list I/J/K/L when 1 gamepad connected; got:\n{text}"
        );
    }

    // ── Four Score: keyboard_target_ports ─────────────────────────────────────

    #[test]
    fn test_keyboard_target_ports_four_score_0_gamepads() {
        // With four-score and 0 gamepads, keyboard covers ports 1 and 2 (same as without).
        assert_eq!(keyboard_target_ports(0, true), &[1, 2]);
    }

    #[test]
    fn test_keyboard_target_ports_four_score_1_gamepad() {
        // Gamepad on port 1; keyboard fills ports 2 and 3.
        assert_eq!(keyboard_target_ports(1, true), &[2, 3]);
    }

    #[test]
    fn test_keyboard_target_ports_four_score_2_gamepads() {
        // Gamepads on ports 1-2; keyboard fills ports 3 and 4.
        assert_eq!(keyboard_target_ports(2, true), &[3, 4]);
    }

    #[test]
    fn test_keyboard_target_ports_four_score_3_gamepads() {
        // Gamepads on ports 1-3; keyboard fills port 4 only.
        assert_eq!(keyboard_target_ports(3, true), &[4]);
    }

    #[test]
    fn test_keyboard_target_ports_four_score_4_gamepads() {
        // All ports owned by gamepads; keyboard disabled.
        assert_eq!(keyboard_target_ports(4, true), &[] as &[u8]);
    }

    #[test]
    fn test_keyboard_target_ports_no_four_score_unchanged() {
        // Without four-score, existing behavior is preserved.
        assert_eq!(keyboard_target_ports(0, false), &[1, 2]);
        assert_eq!(keyboard_target_ports(1, false), &[2]);
        assert_eq!(keyboard_target_ports(2, false), &[] as &[u8]);
        assert_eq!(keyboard_target_ports(3, false), &[] as &[u8]);
    }

    // ── Four Score: keyboard routes P1 keys to port 3 with 2 gamepads ────────

    #[test]
    fn test_wasd_routes_to_port3_with_four_score_and_2_gamepads() {
        let mut console = make_nes_console_four_score();
        let mut state = NativeAppState {
            gamepad_count: 2,
            four_score_enabled: true,
            ..NativeAppState::default()
        };
        handle_key_pressed(&mut console, KeyCode::KeyW, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(3) & BIT_UP,
            0,
            "W should set Up on port 3 with four-score and 2 gamepads"
        );
        assert_eq!(
            console.get_joypad_button_states(1) & BIT_UP,
            0,
            "W should NOT affect port 1 (owned by gamepad)"
        );
        assert_eq!(
            console.get_joypad_button_states(2) & BIT_UP,
            0,
            "W should NOT affect port 2 (owned by gamepad)"
        );
    }

    #[test]
    fn test_ijkl_routes_to_port4_with_four_score_and_2_gamepads() {
        let mut console = make_nes_console_four_score();
        let mut state = NativeAppState {
            gamepad_count: 2,
            four_score_enabled: true,
            ..NativeAppState::default()
        };
        handle_key_pressed(&mut console, KeyCode::KeyI, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(4) & BIT_UP,
            0,
            "I should set Up on port 4 with four-score and 2 gamepads"
        );
    }

    #[test]
    fn test_p2_start_routes_to_port4_with_four_score_and_2_gamepads() {
        let mut console = make_nes_console_four_score();
        let mut state = NativeAppState {
            gamepad_count: 2,
            four_score_enabled: true,
            ..NativeAppState::default()
        };
        handle_key_pressed(&mut console, KeyCode::Digit0, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(4) & BIT_START,
            0,
            "0 should set Start on port 4 with four-score and 2 gamepads"
        );
    }

    #[test]
    fn test_key_release_works_on_port3_with_four_score() {
        let mut console = make_nes_console_four_score();
        let mut state = NativeAppState {
            gamepad_count: 2,
            four_score_enabled: true,
            ..NativeAppState::default()
        };
        handle_key_pressed(&mut console, KeyCode::KeyW, &mut state, None);
        assert_ne!(console.get_joypad_button_states(3) & BIT_UP, 0);
        handle_key_released(&mut console, KeyCode::KeyW, 2, true);
        assert_eq!(
            console.get_joypad_button_states(3) & BIT_UP,
            0,
            "Releasing W should clear Up on port 3"
        );
    }

    // ── Game Boy keyboard dispatch ────────────────────────────────────────────

    /// Creates a minimal valid GB ROM for testing (ROM-only, 32 KB, correct checksum).
    fn minimal_gb_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x8000];
        rom[0x0147] = 0x00; // ROM only
        rom[0x0148] = 0x00; // 32 KB
        rom[0x0149] = 0x00; // no RAM
        let chk = rom[0x0134..=0x014C]
            .iter()
            .fold(0u8, |acc, &b| acc.wrapping_sub(b).wrapping_sub(1));
        rom[0x014D] = chk;
        rom
    }

    fn make_gameboy_console() -> Console {
        let mut console = Console::new_gameboy(AppContext::new_with_config(Config::default()));
        console
            .load_rom(&minimal_gb_rom(), "test.gb")
            .expect("minimal GB ROM should load");
        console
    }

    #[test]
    fn gameboy_d_key_sets_right_button() {
        // Given a Game Boy console and default state (no gamepads)
        let mut console = make_gameboy_console();
        let mut state = make_state();
        // When the 'D' key (Right) is pressed
        handle_key_pressed(&mut console, KeyCode::KeyD, &mut state, None);
        // Then the Right button (bit 7) should be set on port 0
        assert_ne!(
            console.get_joypad_button_states(0) & BIT_RIGHT,
            0,
            "D key should set GB Right button"
        );
    }

    #[test]
    fn gameboy_w_key_sets_up_button() {
        let mut console = make_gameboy_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyW, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(0) & BIT_UP,
            0,
            "W key should set GB Up button"
        );
    }

    #[test]
    fn gameboy_t_key_sets_a_button() {
        let mut console = make_gameboy_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyT, &mut state, None);
        assert_ne!(
            console.get_joypad_button_states(0) & BIT_A,
            0,
            "T key should set GB A button"
        );
    }

    #[test]
    fn gameboy_f6_save_state_returns_continue() {
        let mut console = make_gameboy_console();
        let mut state = make_state();
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::F6, &mut state, None),
            KeyOutcome::Continue,
            "F6 should return Continue in GB mode"
        );
    }

    #[test]
    fn gameboy_f7_load_state_returns_continue() {
        let mut console = make_gameboy_console();
        let mut state = make_state();
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::F7, &mut state, None),
            KeyOutcome::Continue,
            "F7 should return Continue in GB mode"
        );
    }

    #[test]
    fn gameboy_ctrl_r_soft_resets() {
        let mut console = make_gameboy_console();
        let mut state = make_state();
        with_ctrl(&mut state);
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::KeyR, &mut state, None),
            KeyOutcome::Continue,
            "Ctrl+R should return Continue in GB mode"
        );
    }

    #[test]
    fn gameboy_ctrl_shift_r_hard_resets() {
        let mut console = make_gameboy_console();
        let mut state = make_state();
        with_ctrl_shift(&mut state);
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::KeyR, &mut state, None),
            KeyOutcome::Continue,
            "Ctrl+Shift+R should return Continue in GB mode"
        );
    }

    #[test]
    fn snes_f6_save_state_returns_continue() {
        let mut console = make_snes_console("snes-f6-hotkey-test.sfc");
        let mut state = make_state();
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::F6, &mut state, None),
            KeyOutcome::Continue,
            "F6 should return Continue in SNES mode"
        );
    }

    #[test]
    fn snes_f7_load_state_returns_continue() {
        let mut console = make_snes_console("snes-f7-hotkey-test.sfc");
        let mut state = make_state();
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::F7, &mut state, None),
            KeyOutcome::Continue,
            "F7 should return Continue in SNES mode"
        );
    }

    #[test]
    fn snes_f4_cycle_shader_returns_cycle_shader() {
        let mut console = make_snes_console("snes-f4-hotkey-test.sfc");
        let mut state = make_state();
        assert_eq!(
            handle_key_pressed(&mut console, KeyCode::F4, &mut state, None),
            KeyOutcome::CycleShader,
            "F4 should request shader cycling in SNES mode"
        );
    }

    #[test]
    fn gameboy_h_key_toggles_help_overlay_on() {
        let mut console = make_gameboy_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyH, &mut state, None);
        assert!(
            state.help_overlay_visible,
            "H key should toggle help overlay on in GB mode"
        );
    }

    #[test]
    fn gameboy_h_key_toggles_help_overlay_off() {
        let mut console = make_gameboy_console();
        let mut state = make_state();
        state.help_overlay_visible = true;
        handle_key_pressed(&mut console, KeyCode::KeyH, &mut state, None);
        assert!(
            !state.help_overlay_visible,
            "H key should toggle help overlay off in GB mode"
        );
    }

    #[test]
    fn gameboy_d_key_released_clears_right_button() {
        let mut console = make_gameboy_console();
        let mut state = make_state();
        handle_key_pressed(&mut console, KeyCode::KeyD, &mut state, None);
        // Pressing D must set the button first; if not, the release test is meaningless.
        assert_ne!(
            console.get_joypad_button_states(0) & BIT_RIGHT,
            0,
            "D key must set GB Right button before testing release"
        );
        handle_key_released(&mut console, KeyCode::KeyD, 0, false);
        assert_eq!(
            console.get_joypad_button_states(0) & BIT_RIGHT,
            0,
            "Releasing D should clear GB Right button"
        );
    }

    #[test]
    fn gba_keyboard_maps_all_ten_buttons() {
        let cases = [
            (KeyCode::KeyT, GBA_KEY_A, "T should press GBA A"),
            (KeyCode::KeyR, GBA_KEY_B, "R should press GBA B"),
            (KeyCode::Digit4, GBA_KEY_SELECT, "4 should press GBA Select"),
            (KeyCode::Digit5, GBA_KEY_START, "5 should press GBA Start"),
            (KeyCode::KeyW, GBA_KEY_UP, "W should press GBA Up"),
            (KeyCode::KeyS, GBA_KEY_DOWN, "S should press GBA Down"),
            (KeyCode::KeyA, GBA_KEY_LEFT, "A should press GBA Left"),
            (KeyCode::KeyD, GBA_KEY_RIGHT, "D should press GBA Right"),
            (KeyCode::KeyQ, GBA_KEY_L, "Q should press GBA L"),
            (KeyCode::KeyE, GBA_KEY_R, "E should press GBA R"),
            (KeyCode::ArrowUp, GBA_KEY_UP, "ArrowUp should press GBA Up"),
            (
                KeyCode::ArrowDown,
                GBA_KEY_DOWN,
                "ArrowDown should press GBA Down",
            ),
            (
                KeyCode::ArrowLeft,
                GBA_KEY_LEFT,
                "ArrowLeft should press GBA Left",
            ),
            (
                KeyCode::ArrowRight,
                GBA_KEY_RIGHT,
                "ArrowRight should press GBA Right",
            ),
        ];

        for (key, mask, message) in cases {
            let mut console = make_gba_console();
            let mut state = make_state();
            handle_key_pressed(&mut console, key, &mut state, None);
            assert_eq!(gba_keyinput(&console) & mask, 0, "{message}");
        }
    }

    #[test]
    fn gba_keyboard_releases_l_and_r_shoulders() {
        let mut console = make_gba_console();
        let mut state = make_state();

        handle_key_pressed(&mut console, KeyCode::KeyQ, &mut state, None);
        handle_key_pressed(&mut console, KeyCode::KeyE, &mut state, None);

        assert_eq!(
            gba_keyinput(&console) & GBA_KEY_L,
            0,
            "Q should press GBA L"
        );
        assert_eq!(
            gba_keyinput(&console) & GBA_KEY_R,
            0,
            "E should press GBA R"
        );

        handle_key_released(&mut console, KeyCode::KeyQ, 0, false);
        handle_key_released(&mut console, KeyCode::KeyE, 0, false);

        assert_ne!(
            gba_keyinput(&console) & GBA_KEY_L,
            0,
            "releasing Q should clear GBA L"
        );
        assert_ne!(
            gba_keyinput(&console) & GBA_KEY_R,
            0,
            "releasing E should clear GBA R"
        );
    }

    #[test]
    fn gameboy_q_and_e_do_not_map_to_buttons() {
        let mut console = make_gameboy_console();
        let mut state = make_state();

        handle_key_pressed(&mut console, KeyCode::KeyQ, &mut state, None);
        handle_key_pressed(&mut console, KeyCode::KeyE, &mut state, None);

        assert_eq!(
            console.get_joypad_button_states(0),
            0,
            "Game Boy keyboard mapping should remain eight-button only"
        );
    }
}
