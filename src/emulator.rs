//! Hardware-agnostic emulator interface.
//!
//! The [`Console`] enum wraps system-specific emulators (currently only NES)
//! and provides a common interface that frontends can program against.
//! NES-specific features (debugging, PPU viewer, etc.) are accessed by
//! matching on the [`Console::Nes`] variant directly.

use crate::app_context::{IntoSharedAppContext, SharedAppContext};
use crate::nes::cartridge::Cartridge;
use crate::nes::console::Nes;

/// Identifies which emulated system a [`Console`] instance is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemType {
    Nes,
}

/// Hardware-agnostic wrapper around system-specific emulators.
///
/// Frontends use `Console` for common operations (run, render, audio, input,
/// save/load state, reset). System-specific features are accessed by matching
/// on the variant:
///
/// ```ignore
/// if let Console::Nes(nes) = &mut console {
///     // NES-specific: debugger, PPU viewer, Zapper, etc.
/// }
/// ```
pub enum Console {
    Nes(Nes),
}

impl Console {
    /// Create a new NES emulator instance.
    pub fn new_nes(app_context: impl IntoSharedAppContext) -> Self {
        Console::Nes(Nes::new(app_context))
    }

    /// Which system this console is emulating.
    pub fn system_type(&self) -> SystemType {
        match self {
            Console::Nes(_) => SystemType::Nes,
        }
    }

    /// Load a ROM into the emulator.
    ///
    /// For NES, this parses the iNES/NES2.0 header and sets up the mapper.
    /// Uses the console's own `app_context` for ROM database lookups
    /// (auto-detection of controller types, timing modes, etc.).
    pub fn load_rom(&mut self, bytes: &[u8], name: &str) -> Result<(), String> {
        match self {
            Console::Nes(nes) => {
                let app_context = nes.app_context().clone();
                let cart = Cartridge::load_from_file(bytes, name, app_context)
                    .map_err(|e| e.to_string())?;
                nes.insert_cartridge(cart);
                Ok(())
            }
        }
    }

    /// Execute one CPU tick (instruction) and advance all subsystems.
    ///
    /// Returns the number of CPU cycles consumed.
    pub fn run_tick(&mut self) -> u8 {
        match self {
            Console::Nes(nes) => nes.run_cpu_tick(),
        }
    }

    /// Returns `true` when a complete frame has been rendered and is ready
    /// for display.
    pub fn is_ready_to_render(&self) -> bool {
        match self {
            Console::Nes(nes) => nes.is_ready_to_render(),
        }
    }

    /// Clear the frame-ready flag after the frontend has consumed the frame.
    pub fn clear_ready_to_render(&mut self) {
        match self {
            Console::Nes(nes) => nes.clear_ready_to_render(),
        }
    }

    /// Width of the emulated display in pixels.
    pub fn screen_width(&self) -> u32 {
        match self {
            Console::Nes(_) => Nes::SCREEN_WIDTH,
        }
    }

    /// Height of the emulated display in pixels.
    pub fn screen_height(&self) -> u32 {
        match self {
            Console::Nes(_) => Nes::SCREEN_HEIGHT,
        }
    }

    /// Returns a snapshot of the current frame as RGB888 bytes.
    ///
    /// The returned buffer has `screen_width() * screen_height() * 3` bytes,
    /// ordered row-major with 3 bytes per pixel (R, G, B).
    pub fn screen_snapshot(&self) -> Vec<u8> {
        match self {
            Console::Nes(nes) => nes.get_screen_buffer().snapshot(),
        }
    }

    /// Returns a cropped snapshot with overscan removed.
    ///
    /// `h_overscan` pixels are removed from left and right edges.
    /// `v_overscan` pixels are removed from top and bottom edges.
    pub fn cropped_screen_snapshot(&self, h_overscan: u32, v_overscan: u32) -> Vec<u8> {
        match self {
            Console::Nes(nes) => nes
                .get_screen_buffer()
                .cropped_snapshot(h_overscan, v_overscan),
        }
    }

    /// CRC32 of the current screen buffer (for autorun verification).
    pub fn screen_crc32(&self) -> u32 {
        match self {
            Console::Nes(nes) => nes.get_screen_buffer().crc32(),
        }
    }

    /// Returns `true` when an audio sample is ready for retrieval.
    pub fn sample_ready(&self) -> bool {
        match self {
            Console::Nes(nes) => nes.sample_ready(),
        }
    }

    /// Retrieve the next audio sample, if one is ready.
    ///
    /// Returns a sample in the range `0.0..=1.0`, or `None` if no sample
    /// is pending.
    pub fn get_sample(&mut self) -> Option<f32> {
        match self {
            Console::Nes(nes) => nes.get_sample(),
        }
    }

    /// Set a button state on a controller port.
    ///
    /// `button_id` is system-specific: for NES, it maps to [`crate::input::Button`]
    /// discriminant values (A=0, B=1, Select=2, Start=3, Up=4, Down=5, Left=6, Right=7).
    pub fn set_button(&mut self, port: u8, button_id: u8, pressed: bool) {
        match self {
            Console::Nes(nes) => {
                if let Some(button) = nes_button_from_id(button_id) {
                    nes.set_button(port, button, pressed);
                } else {
                    debug_assert!(false, "invalid NES button_id: {button_id}");
                }
            }
        }
    }

    /// Set all button states from a bitmask (for autorun playback).
    ///
    /// Each bit corresponds to a button by its system-specific ID.
    pub fn set_joypad_button_states(&mut self, port: u8, state: u8) {
        match self {
            Console::Nes(nes) => nes.set_joypad_button_states(port, state),
        }
    }

    /// Get all button states as a bitmask (for autorun recording).
    pub fn get_joypad_button_states(&self, port: u8) -> u8 {
        match self {
            Console::Nes(nes) => nes.get_joypad_button_states(port),
        }
    }

    /// Serialize the complete emulator state to bytes.
    pub fn save_state_bytes(&self) -> Result<Vec<u8>, String> {
        match self {
            Console::Nes(nes) => nes
                .save_state()
                .to_bytes()
                .map_err(|e| format!("save state serialization failed: {e}")),
        }
    }

    /// Restore emulator state from previously serialized bytes.
    pub fn load_state_bytes(&mut self, data: &[u8]) -> Result<(), String> {
        match self {
            Console::Nes(nes) => {
                let state = crate::nes::console::SaveState::from_bytes(data)
                    .map_err(|e| format!("save state deserialization failed: {e}"))?;
                nes.load_state(&state).map_err(|e| e.to_string())
            }
        }
    }

    /// Reset the emulator.
    ///
    /// `soft_reset` = true simulates pressing the reset button.
    /// `soft_reset` = false simulates a power cycle.
    pub fn reset(&mut self, soft_reset: bool) {
        match self {
            Console::Nes(nes) => nes.reset(soft_reset),
        }
    }

    /// Access the shared application context (config, ROM database, toasts).
    pub fn app_context(&self) -> &SharedAppContext {
        match self {
            Console::Nes(nes) => nes.app_context(),
        }
    }

    /// Save battery-backed RAM to disk (if applicable).
    pub fn save_ram(&self) -> Result<(), String> {
        match self {
            Console::Nes(nes) => nes.save_ram().map_err(|e| e.to_string()),
        }
    }

    /// Set the audio output sample rate (Hz) for the emulator's APU.
    pub fn set_audio_sample_rate(&mut self, rate: f32) {
        match self {
            Console::Nes(nes) => nes.set_audio_sample_rate(rate),
        }
    }
}

fn nes_button_from_id(id: u8) -> Option<crate::input::Button> {
    use crate::input::Button;
    match id {
        0 => Some(Button::A),
        1 => Some(Button::B),
        2 => Some(Button::Select),
        3 => Some(Button::Start),
        4 => Some(Button::Up),
        5 => Some(Button::Down),
        6 => Some(Button::Left),
        7 => Some(Button::Right),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_context::AppContext;
    use crate::nes::console::Config;

    /// Create a minimal valid iNES ROM for testing.
    fn create_minimal_rom() -> Vec<u8> {
        let mut rom = Vec::new();
        rom.extend_from_slice(b"NES\x1A");
        rom.push(1); // 1x 16 KB PRG ROM
        rom.push(0); // No CHR ROM
        rom.push(0); // Flags 6
        rom.push(0); // Flags 7
        rom.extend_from_slice(&[0; 8]); // Padding

        let mut prg_rom = vec![0; 16384];
        // Reset vector at $FFFC-$FFFD → $8000
        prg_rom[0x3FFC] = 0x00;
        prg_rom[0x3FFD] = 0x80;
        // JMP $8000 (infinite loop)
        prg_rom[0] = 0x4C;
        prg_rom[1] = 0x00;
        prg_rom[2] = 0x80;

        rom.extend_from_slice(&prg_rom);
        rom
    }

    fn make_console() -> Console {
        let config = Config::default();
        let app_context = AppContext::new_with_config(config);
        Console::new_nes(app_context)
    }

    fn make_console_with_rom() -> (Console, SharedAppContext) {
        let config = Config::default();
        let app_context: SharedAppContext = AppContext::new_with_config(config).into_shared();
        let mut console = Console::new_nes(app_context.clone());
        let rom = create_minimal_rom();
        console.load_rom(&rom, "test.nes").expect("load ROM");
        console.reset(false);
        (console, app_context)
    }

    #[test]
    fn test_system_type_returns_nes() {
        let console = make_console();
        assert_eq!(console.system_type(), SystemType::Nes);
    }

    #[test]
    fn test_screen_dimensions_are_nes() {
        let console = make_console();
        assert_eq!(console.screen_width(), 256);
        assert_eq!(console.screen_height(), 240);
    }

    #[test]
    fn test_screen_snapshot_has_correct_size() {
        let console = make_console();
        let snapshot = console.screen_snapshot();
        assert_eq!(snapshot.len(), 256 * 240 * 3);
    }

    #[test]
    fn test_is_ready_to_render_initially_false() {
        let console = make_console();
        assert!(!console.is_ready_to_render());
    }

    #[test]
    fn test_run_tick_returns_nonzero_cycles() {
        let (mut console, _) = make_console_with_rom();
        let cycles = console.run_tick();
        assert!(cycles > 0);
    }

    #[test]
    fn test_reset_does_not_panic() {
        let (mut console, _) = make_console_with_rom();
        console.reset(false);
        console.reset(true);
    }

    #[test]
    fn test_set_button_does_not_panic() {
        let mut console = make_console();
        // NES Button::A = 0
        console.set_button(1, 0, true);
        console.set_button(1, 0, false);
    }

    #[test]
    fn test_save_and_load_state_roundtrip() {
        let (mut console, _) = make_console_with_rom();

        let state_bytes = console.save_state_bytes().expect("save should succeed");
        assert!(!state_bytes.is_empty());

        let result = console.load_state_bytes(&state_bytes);
        assert!(result.is_ok(), "load should succeed: {:?}", result.err());
    }

    #[test]
    fn test_load_state_with_invalid_bytes_returns_error() {
        let (mut console, _) = make_console_with_rom();
        let result = console.load_state_bytes(b"not valid state data");
        assert!(result.is_err());
    }

    #[test]
    fn test_nes_variant_is_accessible() {
        let mut console = make_console();
        let Console::Nes(nes) = &mut console;
        assert!(!nes.is_ready_to_render());
    }

    #[test]
    fn test_joypad_states_roundtrip() {
        let mut console = make_console();
        console.set_joypad_button_states(1, 0b1010_0101);
        let state = console.get_joypad_button_states(1);
        assert_eq!(state, 0b1010_0101);
    }

    #[test]
    fn test_screen_crc32_for_blank_frame() {
        let (console, _) = make_console_with_rom();
        let crc = console.screen_crc32();
        assert_ne!(crc, 0, "CRC should be non-zero even for a blank screen");
    }

    #[test]
    fn test_load_rom_with_invalid_data_returns_error() {
        let mut console = Console::new_nes(AppContext::new_with_config(Config::default()));
        let result = console.load_rom(b"not a valid ROM", "bad.nes");
        assert!(result.is_err());
    }

    #[test]
    fn test_run_until_frame_ready() {
        let (mut console, _) = make_console_with_rom();
        let mut total_cycles = 0u64;
        while !console.is_ready_to_render() && total_cycles < 100_000 {
            total_cycles += console.run_tick() as u64;
        }
        assert!(
            console.is_ready_to_render(),
            "Frame should be ready after running enough cycles"
        );
        console.clear_ready_to_render();
        assert!(!console.is_ready_to_render());
    }

    // ---------------------------------------------------------------
    // Trait-based extensibility proof — shows the Console interface
    // can support multiple hardware targets via a common trait.
    // If Console adds a method that should be generic, adding it to
    // this trait will produce a compile error until GameBoyStub
    // implements it, catching maintenance gaps at build time.
    // ---------------------------------------------------------------

    /// Common operations that every emulated system must support.
    trait SystemOps {
        fn run_tick(&mut self) -> u8;
        fn is_ready_to_render(&self) -> bool;
        fn screen_width(&self) -> u32;
        fn screen_height(&self) -> u32;
        fn screen_snapshot(&self) -> Vec<u8>;
    }

    impl SystemOps for Console {
        fn run_tick(&mut self) -> u8 {
            Console::run_tick(self)
        }
        fn is_ready_to_render(&self) -> bool {
            Console::is_ready_to_render(self)
        }
        fn screen_width(&self) -> u32 {
            Console::screen_width(self)
        }
        fn screen_height(&self) -> u32 {
            Console::screen_height(self)
        }
        fn screen_snapshot(&self) -> Vec<u8> {
            Console::screen_snapshot(self)
        }
    }

    /// Minimal stub proving a second system can implement the same trait.
    struct GameBoyStub {
        frame_ready: bool,
        cycles_in_frame: u32,
        screen: Vec<u8>,
    }

    impl GameBoyStub {
        const WIDTH: u32 = 160;
        const HEIGHT: u32 = 144;
        const CYCLES_PER_FRAME: u32 = 70224; // ~4.19 MHz / ~59.73 Hz

        fn new() -> Self {
            Self {
                frame_ready: false,
                cycles_in_frame: 0,
                screen: vec![0; (Self::WIDTH * Self::HEIGHT * 3) as usize],
            }
        }
    }

    impl SystemOps for GameBoyStub {
        fn run_tick(&mut self) -> u8 {
            let cycles = 4; // GB instructions are 4-cycle minimum
            self.cycles_in_frame += cycles as u32;
            if self.cycles_in_frame >= Self::CYCLES_PER_FRAME {
                self.cycles_in_frame -= Self::CYCLES_PER_FRAME;
                self.frame_ready = true;
            }
            cycles
        }
        fn is_ready_to_render(&self) -> bool {
            self.frame_ready
        }
        fn screen_width(&self) -> u32 {
            Self::WIDTH
        }
        fn screen_height(&self) -> u32 {
            Self::HEIGHT
        }
        fn screen_snapshot(&self) -> Vec<u8> {
            self.screen.clone()
        }
    }

    fn run_system_to_frame(system: &mut dyn SystemOps) -> u64 {
        let mut total = 0u64;
        while !system.is_ready_to_render() && total < 200_000 {
            total += system.run_tick() as u64;
        }
        total
    }

    #[test]
    fn test_gameboy_stub_via_trait() {
        let mut gb = GameBoyStub::new();
        assert_eq!(gb.screen_width(), 160);
        assert_eq!(gb.screen_height(), 144);

        run_system_to_frame(&mut gb);
        assert!(gb.is_ready_to_render());
        assert_eq!(gb.screen_snapshot().len(), (160 * 144 * 3) as usize);
    }

    #[test]
    fn test_console_via_trait() {
        let (mut console, _) = make_console_with_rom();
        assert_eq!(console.screen_width(), 256);
        assert_eq!(console.screen_height(), 240);

        run_system_to_frame(&mut console);
        assert!(console.is_ready_to_render());
    }
}
