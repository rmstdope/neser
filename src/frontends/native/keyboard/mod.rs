//! Keyboard input and hotkey handling for the native frontend.
//!
//! Exposes the entry points ([`handle_key_pressed`], [`handle_key_released`],
//! [`keyboard_target_ports`], [`KeyOutcome`]) and delegates to per-domain
//! submodules: [`hotkeys`] (system/debugger/cartridge-switch hotkeys),
//! [`console_keyboard`] (per-console press dispatch), and [`controller_mapping`]
//! (key→button mapping tables).

use crate::frontends::native::app_state::NativeAppState;
use crate::platform::audio::EmulatorAudio;
use crate::platform::emulator::{Console, SystemType};
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
    match console.system_type() {
        SystemType::Nes => {
            if app_state.cart_switch.open {
                return hotkeys::handle_cartridge_switch_key(key_code, app_state);
            }
            if app_state.modifiers.control_key() {
                return hotkeys::handle_ctrl_hotkey(console, key_code, app_state);
            }
            console_keyboard::handle_unmodified_key(console, key_code, app_state, audio)
        }
        SystemType::GameBoy => {
            console_keyboard::handle_gameboy_key_pressed(console, key_code, app_state, audio)
        }
        SystemType::Gba => {
            console_keyboard::handle_gba_key_pressed(console, key_code, app_state, audio)
        }
        SystemType::Snes => {
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
    match console.system_type() {
        SystemType::Nes => {
            let ports = keyboard_target_ports(gamepad_count, four_score);
            controller_mapping::handle_controller_key(console, key_code, false, ports);
        }
        SystemType::GameBoy => {
            if let Some(btn_id) = controller_mapping::gameboy_key_to_button_id(key_code) {
                console.set_button(0, btn_id, false);
            }
        }
        SystemType::Gba => {
            if let Some(btn_id) = controller_mapping::gba_key_to_button_id(key_code) {
                console.set_button(0, btn_id, false);
            }
        }
        SystemType::Snes => {
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
pub(crate) mod test_support {
    use crate::frontends::native::app_state::NativeAppState;
    use crate::nes::console::{Config, Nes, NesConfig};
    use crate::nes::input::Button;
    use crate::platform::app_context::AppContext;
    use crate::platform::audio::EmulatorAudio;
    use crate::platform::emulator::Console;
    use winit::keyboard::ModifiersState;

    pub(crate) fn make_nes() -> Nes {
        Nes::new(AppContext::new_with_config(Config::default()))
    }

    pub(crate) fn make_nes_four_score() -> Nes {
        Nes::new(AppContext::new_with_config(Config {
            nes: NesConfig {
                four_score_enabled: true,
                ..Default::default()
            },
            ..Config::default()
        }))
    }

    pub(crate) fn make_nes_with_cartridge() -> Nes {
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

    pub(crate) fn make_nes_console() -> Console {
        Console::Nes(Box::new(make_nes()))
    }

    pub(crate) fn make_nes_console_with_cart() -> Console {
        Console::Nes(Box::new(make_nes_with_cartridge()))
    }

    pub(crate) fn make_nes_console_four_score() -> Console {
        Console::Nes(Box::new(make_nes_four_score()))
    }

    pub(crate) fn make_gba_console() -> Console {
        Console::new_gba(AppContext::new_with_config(Config::default()))
    }

    pub(crate) fn minimal_snes_rom() -> Vec<u8> {
        let mut rom = vec![0u8; 0x10000];
        let header = 0x7FC0;
        rom[header..header + 21].copy_from_slice(b"SNES TEST ROM        ");
        rom[header + 0x3C] = 0x00;
        rom[header + 0x3D] = 0x80;
        rom[header + 0x15] = 0x20;
        rom[header + 0x16] = 0x00;
        rom[header + 0x17] = 0x07;
        rom[header + 0x18] = 0x00;
        rom[header + 0x19] = 0x00;
        rom[header + 0x1C] = 0x34;
        rom[header + 0x1D] = 0x12;
        rom[header + 0x1E] = 0xCB;
        rom[header + 0x1F] = 0xED;
        rom[0x0000] = 0xEA;
        rom
    }

    pub(crate) fn make_snes_console(rom_name: &str) -> Console {
        let mut console = Console::new_snes(crate::snes::test_support::snes_test_app_context());
        console
            .load_rom(&minimal_snes_rom(), rom_name)
            .expect("minimal SNES ROM should load");
        console
    }

    pub(crate) fn gba_keyinput(console: &Console) -> u16 {
        let Console::GameBoyAdvance(gba) = console else {
            panic!("expected GBA console");
        };
        gba.bus().keypad.read_keyinput()
    }

    pub(crate) fn make_state() -> NativeAppState {
        NativeAppState::default()
    }

    pub(crate) fn with_ctrl(state: &mut NativeAppState) {
        state.modifiers = ModifiersState::CONTROL;
    }

    pub(crate) fn with_ctrl_shift(state: &mut NativeAppState) {
        state.modifiers = ModifiersState::CONTROL | ModifiersState::SHIFT;
    }

    #[allow(dead_code)]
    pub(crate) fn buttons(nes: &Nes, port: u8) -> u8 {
        nes.get_joypad_button_states(port)
    }

    pub(crate) const BIT_A: u8 = 1 << Button::A as u8;
    pub(crate) const BIT_B: u8 = 1 << Button::B as u8;
    pub(crate) const BIT_SELECT: u8 = 1 << Button::Select as u8;
    pub(crate) const BIT_START: u8 = 1 << Button::Start as u8;
    pub(crate) const BIT_UP: u8 = 1 << Button::Up as u8;
    pub(crate) const BIT_DOWN: u8 = 1 << Button::Down as u8;
    pub(crate) const BIT_LEFT: u8 = 1 << Button::Left as u8;
    pub(crate) const BIT_RIGHT: u8 = 1 << Button::Right as u8;

    pub(crate) const GBA_KEY_A: u16 = 1 << 0;
    pub(crate) const GBA_KEY_B: u16 = 1 << 1;
    pub(crate) const GBA_KEY_SELECT: u16 = 1 << 2;
    pub(crate) const GBA_KEY_START: u16 = 1 << 3;
    pub(crate) const GBA_KEY_RIGHT: u16 = 1 << 4;
    pub(crate) const GBA_KEY_LEFT: u16 = 1 << 5;
    pub(crate) const GBA_KEY_UP: u16 = 1 << 6;
    pub(crate) const GBA_KEY_DOWN: u16 = 1 << 7;
    pub(crate) const GBA_KEY_R: u16 = 1 << 8;
    pub(crate) const GBA_KEY_L: u16 = 1 << 9;

    // ── Mock audio ────────────────────────────────────────────────────────────

    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    pub(crate) struct MockAudio {
        volume: Arc<AtomicU32>,
    }

    /// Mock audio that tracks whether pause(), resume(), and drain_buffer() have been called.
    pub(crate) struct TrackingMockAudio {
        pause_called: Arc<AtomicBool>,
        resume_called: Arc<AtomicBool>,
        drain_buffer_called: Arc<AtomicBool>,
    }

    impl TrackingMockAudio {
        pub(crate) fn new() -> (Self, Arc<AtomicBool>, Arc<AtomicBool>, Arc<AtomicBool>) {
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
        pub(crate) fn new_with_volume(vol: f32) -> Self {
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
}

#[cfg(test)]
mod tests {

    use super::*;

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
}
