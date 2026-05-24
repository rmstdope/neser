//! Hardware-agnostic emulator interface.
//!
//! The [`Console`] enum wraps system-specific emulators (currently only NES)
//! and provides a common interface that frontends can program against.
//! NES-specific features (debugging, PPU viewer, etc.) are accessed by
//! matching on the [`Console::Nes`] variant directly.
//!
//! The [`Emulator`] trait defines the common operations every emulated system
//! must support.  `Console` delegates its common methods through
//! [`as_core()`](Console::as_core) / [`as_core_mut()`](Console::as_core_mut),
//! keeping a single pair of match arms instead of one pair per method.

use std::path::PathBuf;

use crate::gb::GameBoy;
use crate::gba::Gba;
use crate::nes::console::Nes;
use crate::platform::app_context::{IntoSharedAppContext, SharedAppContext};

/// Common operations that every emulated system must support.
///
/// Frontends program against this trait (via [`Console`]) for system-agnostic
/// functionality: execution, rendering, audio, input, state management, and
/// configuration.  System-specific features are accessed by downcasting the
/// [`Console`] variant.
pub trait Emulator {
    fn system_type(&self) -> SystemType;
    /// Short names of shader presets permitted for this emulator.
    ///
    /// Names correspond to the first element of each entry in
    /// [`crate::platform::shaders::SHADER_PRESETS`].
    fn allowed_shaders(&self) -> &'static [&'static str];
    fn load_rom(&mut self, bytes: &[u8], name: &str) -> Result<(), String>;
    fn run_tick(&mut self) -> u8;
    fn is_ready_to_render(&self) -> bool;
    fn clear_ready_to_render(&mut self);
    fn screen_width(&self) -> u32;
    fn screen_height(&self) -> u32;
    fn screen_snapshot(&self) -> Vec<u8>;
    fn cropped_screen_snapshot(&self, h_overscan: u32, v_overscan: u32) -> Vec<u8>;
    fn screen_crc32(&self) -> u32;
    fn sample_ready(&self) -> bool;
    fn get_sample(&mut self) -> Option<f32>;
    /// Returns the next stereo audio sample as `(left, right)` pair.
    ///
    /// The default implementation calls [`get_sample`](Self::get_sample) and
    /// duplicates the mono result to both channels.  Emulators that produce
    /// true stereo (e.g. GBA) should override this.
    fn get_stereo_sample(&mut self) -> Option<(f32, f32)> {
        self.get_sample().map(|s| (s, s))
    }
    fn set_audio_sample_rate(&mut self, rate: f32);
    fn set_button(&mut self, port: u8, button_id: u8, pressed: bool);
    fn set_joypad_button_states(&mut self, port: u8, state: u8);
    fn get_joypad_button_states(&self, port: u8) -> u8;
    fn save_state_bytes(&self) -> Result<Vec<u8>, String>;
    fn load_state_bytes(&mut self, data: &[u8]) -> Result<(), String>;
    fn reset(&mut self, soft_reset: bool);
    fn save_ram(&self) -> Result<(), String>;
    fn app_context(&self) -> &SharedAppContext;
    fn target_frame_duration(&self) -> std::time::Duration;
}

/// Identifies which emulated system a [`Console`] instance is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemType {
    Nes,
    GameBoy,
    Gba,
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
    Nes(Box<Nes>),
    GameBoy(Box<GameBoy>),
    GameBoyAdvance(Box<Gba>),
}

impl Console {
    /// Create a new NES emulator instance.
    pub fn new_nes(app_context: impl IntoSharedAppContext) -> Self {
        Console::Nes(Box::new(Nes::new(app_context)))
    }

    /// Create a new Game Boy (DMG) emulator instance.
    pub fn new_gameboy(app_context: impl IntoSharedAppContext) -> Self {
        Console::GameBoy(Box::new(GameBoy::new(app_context)))
    }

    /// Create a new Game Boy Advance emulator instance.
    pub fn new_gba(app_context: impl IntoSharedAppContext) -> Self {
        Console::GameBoyAdvance(Box::new(Gba::new(app_context)))
    }

    /// Access the common emulator interface (immutable).
    pub fn as_core(&self) -> &dyn Emulator {
        match self {
            Console::Nes(nes) => nes.as_ref(),
            Console::GameBoy(gb) => gb.as_ref(),
            Console::GameBoyAdvance(gba) => gba.as_ref(),
        }
    }

    /// Access the common emulator interface (mutable).
    pub fn as_core_mut(&mut self) -> &mut dyn Emulator {
        match self {
            Console::Nes(nes) => nes.as_mut(),
            Console::GameBoy(gb) => gb.as_mut(),
            Console::GameBoyAdvance(gba) => gba.as_mut(),
        }
    }

    /// Which system this console is emulating.
    pub fn system_type(&self) -> SystemType {
        self.as_core().system_type()
    }

    /// Short names of shader presets permitted for this console's emulator.
    pub fn allowed_shaders(&self) -> &'static [&'static str] {
        self.as_core().allowed_shaders()
    }

    /// Load a ROM into the emulator.
    ///
    /// For NES, this parses the iNES/NES2.0 header and sets up the mapper.
    /// Uses the console's own `app_context` for ROM database lookups
    /// (auto-detection of controller types, timing modes, etc.).
    ///
    /// Note: inserts the cartridge directly. For startup flows that need
    /// to inspect the cartridge before insertion (timing mode, toasts),
    /// destructure the Console variant and use `insert_cartridge` directly.
    pub fn load_rom(&mut self, bytes: &[u8], name: &str) -> Result<(), String> {
        self.as_core_mut().load_rom(bytes, name)
    }

    /// Execute one CPU tick (instruction) and advance all subsystems.
    ///
    /// Returns the number of CPU cycles consumed.
    pub fn run_tick(&mut self) -> u8 {
        self.as_core_mut().run_tick()
    }

    /// Returns `true` when a complete frame has been rendered and is ready
    /// for display.
    pub fn is_ready_to_render(&self) -> bool {
        self.as_core().is_ready_to_render()
    }

    /// Clear the frame-ready flag after the frontend has consumed the frame.
    pub fn clear_ready_to_render(&mut self) {
        self.as_core_mut().clear_ready_to_render()
    }

    /// Width of the emulated display in pixels.
    pub fn screen_width(&self) -> u32 {
        self.as_core().screen_width()
    }

    /// Height of the emulated display in pixels.
    pub fn screen_height(&self) -> u32 {
        self.as_core().screen_height()
    }

    /// Returns a snapshot of the current frame as RGB888 bytes.
    ///
    /// The returned buffer has `screen_width() * screen_height() * 3` bytes,
    /// ordered row-major with 3 bytes per pixel (R, G, B).
    pub fn screen_snapshot(&self) -> Vec<u8> {
        self.as_core().screen_snapshot()
    }

    /// Returns a cropped snapshot with overscan removed.
    ///
    /// `h_overscan` pixels are removed from left and right edges.
    /// `v_overscan` pixels are removed from top and bottom edges.
    pub fn cropped_screen_snapshot(&self, h_overscan: u32, v_overscan: u32) -> Vec<u8> {
        self.as_core()
            .cropped_screen_snapshot(h_overscan, v_overscan)
    }

    /// CRC32 of the current screen buffer (for autorun verification).
    pub fn screen_crc32(&self) -> u32 {
        self.as_core().screen_crc32()
    }

    /// Returns `true` when an audio sample is ready for retrieval.
    pub fn sample_ready(&self) -> bool {
        self.as_core().sample_ready()
    }

    /// Retrieve the next audio sample, if one is ready.
    ///
    /// Returns a sample in the range `0.0..=1.0`, or `None` if no sample
    /// is pending.
    pub fn get_sample(&mut self) -> Option<f32> {
        self.as_core_mut().get_sample()
    }

    /// Retrieve the next audio sample as a stereo `(left, right)` pair.
    ///
    /// Returns `None` if no sample is pending.  For mono emulators (NES, GB),
    /// both channels carry the same value.  For GBA, channels are independent.
    pub fn get_stereo_sample(&mut self) -> Option<(f32, f32)> {
        self.as_core_mut().get_stereo_sample()
    }

    /// Set a button state on a controller port.
    ///
    /// `button_id` is system-specific: for NES, it maps to [`crate::nes::input::Button`]
    /// discriminant values (A=0, B=1, Select=2, Start=3, Up=4, Down=5, Left=6, Right=7).
    /// For Game Boy, only `port == 0` is meaningful; calls for other ports are ignored.
    pub fn set_button(&mut self, port: u8, button_id: u8, pressed: bool) {
        self.as_core_mut().set_button(port, button_id, pressed)
    }

    /// Set all button states from a bitmask (for autorun playback).
    ///
    /// Each bit corresponds to a button by its system-specific ID.
    /// For Game Boy, only `port == 0` is applied; other ports are ignored.
    pub fn set_joypad_button_states(&mut self, port: u8, state: u8) {
        self.as_core_mut().set_joypad_button_states(port, state)
    }

    /// Get all button states as a bitmask (for autorun recording).
    ///
    /// For Game Boy, only `port == 0` returns button states; other ports return 0.
    pub fn get_joypad_button_states(&self, port: u8) -> u8 {
        self.as_core().get_joypad_button_states(port)
    }

    /// Serialize the complete emulator state to bytes.
    pub fn save_state_bytes(&self) -> Result<Vec<u8>, String> {
        self.as_core().save_state_bytes()
    }

    /// Restore emulator state from previously serialized bytes.
    pub fn load_state_bytes(&mut self, data: &[u8]) -> Result<(), String> {
        self.as_core_mut().load_state_bytes(data)
    }

    /// Returns the file path where save-state data should be stored, or `None`
    /// if no ROM is loaded or the system does not support disk save states.
    pub fn state_path(&self) -> Option<PathBuf> {
        match self {
            Console::Nes(nes) => nes.state_path(),
            Console::GameBoy(gb) => gb.state_path(),
            Console::GameBoyAdvance(gba) => gba.state_path(),
        }
    }

    /// Reset the emulator.
    ///
    /// `soft_reset` = true simulates pressing the reset button.
    /// `soft_reset` = false simulates a power cycle.
    pub fn reset(&mut self, soft_reset: bool) {
        self.as_core_mut().reset(soft_reset)
    }

    /// Access the shared application context (config, ROM database, toasts).
    pub fn app_context(&self) -> &SharedAppContext {
        self.as_core().app_context()
    }

    /// Save battery-backed RAM to disk (if applicable).
    pub fn save_ram(&self) -> Result<(), String> {
        self.as_core().save_ram()
    }

    /// Set the audio output sample rate (Hz) for the emulator's APU.
    pub fn set_audio_sample_rate(&mut self, rate: f32) {
        self.as_core_mut().set_audio_sample_rate(rate)
    }

    /// Horizontal and vertical overscan in pixels for the current system.
    ///
    /// For NES, values are read from the emulator configuration.
    /// For Game Boy, overscan is always `(0, 0)` — the GB has no overscan.
    pub fn overscan(&self) -> (u32, u32) {
        match self {
            Console::Nes(nes) => {
                let ctx = nes.app_context().borrow();
                let cfg = ctx.config();
                (
                    cfg.nes.horizontal_overscan as u32,
                    cfg.nes.vertical_overscan as u32,
                )
            }
            Console::GameBoy(_) | Console::GameBoyAdvance(_) => (0, 0),
        }
    }

    /// Visible screen dimensions in pixels with overscan removed.
    ///
    /// For NES, `h_overscan` columns are removed from each side and `v_overscan`
    /// rows from top and bottom.
    ///
    /// # Panics
    ///
    /// Panics for NES if `h_overscan` exceeds half the screen width or
    /// `v_overscan` exceeds half the screen height. This matches the stricter
    /// precondition used by the NES cropped snapshot path instead of silently
    /// clamping invalid values.
    ///
    /// For Game Boy, overscan parameters are ignored and the native 160×144
    /// resolution is always returned.
    pub fn cropped_dims(&self, h_overscan: u32, v_overscan: u32) -> (u32, u32) {
        match self {
            Console::Nes(_) => {
                let screen_width = self.screen_width();
                let screen_height = self.screen_height();
                let max_h_overscan = screen_width / 2;
                let max_v_overscan = screen_height / 2;

                assert!(
                    h_overscan <= max_h_overscan,
                    "horizontal overscan {} exceeds maximum {} for width {}",
                    h_overscan,
                    max_h_overscan,
                    screen_width
                );
                assert!(
                    v_overscan <= max_v_overscan,
                    "vertical overscan {} exceeds maximum {} for height {}",
                    v_overscan,
                    max_v_overscan,
                    screen_height
                );

                (
                    screen_width - 2 * h_overscan,
                    screen_height - 2 * v_overscan,
                )
            }
            Console::GameBoy(_) | Console::GameBoyAdvance(_) => {
                (self.screen_width(), self.screen_height())
            }
        }
    }

    /// Pixel aspect ratio correction factor for the current system.
    ///
    /// NES pixels are not square: the NTSC hardware maps 256 pixels across the
    /// same horizontal extent as approximately 280 square pixels (8:7 ratio).
    /// Game Boy and GBA pixels are square, so the correction factor is 1.0.
    pub fn pixel_aspect(&self) -> f32 {
        match self {
            Console::Nes(_) => 8.0 / 7.0,
            Console::GameBoy(_) | Console::GameBoyAdvance(_) => 1.0,
        }
    }

    /// Target wall-clock duration between rendered frames for this system.
    ///
    /// Frontends use this to pace emulation correctly regardless of display
    /// refresh rate.  The NES value is derived from the hardware timing mode
    /// (NTSC ≈ 60.10 fps, PAL ≈ 50.01 fps).  The DMG Game Boy always runs at
    /// 4,194,304 Hz / 70,224 cycles per frame ≈ 59.73 fps.
    pub fn target_frame_duration(&self) -> std::time::Duration {
        self.as_core().target_frame_duration()
    }
}

impl SystemType {
    /// Computes windowed-mode dimensions that preserve the system's correct aspect ratio.
    ///
    /// For NES, overscan is read from `app_context` and the NTSC 8:7 pixel aspect
    /// ratio is applied.  For Game Boy and GBA, square pixels (1:1) are assumed
    /// and there is no overscan.
    pub fn windowed_dimensions(&self, height: u32, app_context: &SharedAppContext) -> (u32, u32) {
        let clamped_height = height.max(1);
        match self {
            SystemType::Nes => {
                let ctx = app_context.borrow();
                let cfg = ctx.config();
                let h_overscan = cfg.nes.horizontal_overscan as u32;
                let v_overscan = cfg.nes.vertical_overscan as u32;
                let visible_w = Nes::SCREEN_WIDTH.saturating_sub(2 * h_overscan).max(1) as f32;
                let visible_h = Nes::SCREEN_HEIGHT.saturating_sub(2 * v_overscan).max(1) as f32;
                let aspect = (visible_w / visible_h) * (8.0 / 7.0);
                let width = (clamped_height as f32 * aspect).round() as u32;
                (width.max(1), clamped_height)
            }
            SystemType::GameBoy => {
                let aspect = GameBoy::SCREEN_WIDTH as f32 / GameBoy::SCREEN_HEIGHT as f32;
                let width = (clamped_height as f32 * aspect).round() as u32;
                (width.max(1), clamped_height)
            }
            SystemType::Gba => {
                // GBA: 240×160 with square pixels (1:1)
                let aspect = Gba::SCREEN_WIDTH as f32 / Gba::SCREEN_HEIGHT as f32;
                let width = (clamped_height as f32 * aspect).round() as u32;
                (width.max(1), clamped_height)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::console::Config;
    use crate::platform::app_context::AppContext;

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

    fn make_shared_context() -> SharedAppContext {
        AppContext::new_with_config(Config::default()).into_shared()
    }

    fn make_console() -> Console {
        Console::new_nes(make_shared_context())
    }

    fn make_nes() -> Nes {
        Nes::new(make_shared_context())
    }

    fn make_gameboy() -> GameBoy {
        GameBoy::new(make_shared_context())
    }

    fn make_console_with_rom() -> (Console, SharedAppContext) {
        let app_context = make_shared_context();
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
        if let Console::Nes(nes) = &mut console {
            assert!(!nes.is_ready_to_render());
        } else {
            panic!("expected Console::Nes");
        }
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
        let mut console = make_console();
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
    // Emulator-trait tests — verifies that Nes and GameBoy both
    // implement the production Emulator trait and that Console
    // delegates through as_core()/as_core_mut().
    // ---------------------------------------------------------------

    fn run_emulator_to_frame(emu: &mut dyn Emulator) -> u64 {
        let mut total = 0u64;
        while !emu.is_ready_to_render() && total < 200_000 {
            let ticks = emu.run_tick() as u64;
            if ticks == 0 {
                break;
            }
            total += ticks;
        }
        total
    }

    #[test]
    fn test_nes_implements_emulator_trait() {
        let mut nes = make_nes();
        let rom = create_minimal_rom();
        nes.load_rom(&rom, "test.nes").unwrap();
        nes.reset(false);

        let emu: &mut dyn Emulator = &mut nes;
        assert_eq!(emu.system_type(), SystemType::Nes);
        assert_eq!(emu.screen_width(), 256);
        assert_eq!(emu.screen_height(), 240);
        assert!(!emu.is_ready_to_render());

        run_emulator_to_frame(emu);
        assert!(emu.is_ready_to_render());
        emu.clear_ready_to_render();
        assert!(!emu.is_ready_to_render());
    }

    #[test]
    fn test_gameboy_implements_emulator_trait() {
        let mut gb = make_gameboy();

        let emu: &dyn Emulator = &gb;
        assert_eq!(emu.system_type(), SystemType::GameBoy);
        assert_eq!(emu.screen_width(), 160);
        assert_eq!(emu.screen_height(), 144);

        // No ROM loaded — safe default behaviour
        let emu: &mut dyn Emulator = &mut gb;
        assert_eq!(emu.run_tick(), 0);
        assert!(!emu.is_ready_to_render());
    }

    #[test]
    fn test_console_as_core_delegates_to_nes() {
        let (mut console, _) = make_console_with_rom();

        let emu = console.as_core();
        assert_eq!(emu.system_type(), SystemType::Nes);
        assert_eq!(emu.screen_width(), 256);
        assert_eq!(emu.screen_height(), 240);

        let emu = console.as_core_mut();
        run_emulator_to_frame(emu);
        assert!(console.as_core().is_ready_to_render());
    }

    #[test]
    fn test_console_as_core_delegates_to_gameboy() {
        let console = Console::new_gameboy(make_shared_context());

        let emu = console.as_core();
        assert_eq!(emu.system_type(), SystemType::GameBoy);
        assert_eq!(emu.screen_width(), 160);
        assert_eq!(emu.screen_height(), 144);
    }

    #[test]
    fn test_nes_trait_screen_snapshot_has_correct_size() {
        let nes = make_nes();
        let emu: &dyn Emulator = &nes;
        assert_eq!(emu.screen_snapshot().len(), 256 * 240 * 3);
    }

    #[test]
    fn test_gameboy_trait_screen_snapshot_has_correct_size() {
        let gb = make_gameboy();
        let emu: &dyn Emulator = &gb;
        assert_eq!(emu.screen_snapshot().len(), 160 * 144 * 3);
    }

    #[test]
    fn test_nes_trait_target_frame_duration() {
        let nes = make_nes();
        let emu: &dyn Emulator = &nes;
        let dur = emu.target_frame_duration();
        // NTSC ≈ 60.10 fps → ~16.6 ms
        assert!(dur.as_millis() > 15 && dur.as_millis() < 20);
    }

    #[test]
    fn test_gameboy_trait_target_frame_duration() {
        let gb = make_gameboy();
        let emu: &dyn Emulator = &gb;
        let dur = emu.target_frame_duration();
        // DMG ≈ 59.73 fps → ~16.7 ms
        assert!(dur.as_millis() > 15 && dur.as_millis() < 20);
    }

    #[test]
    fn test_nes_trait_save_ram_without_cart_returns_ok() {
        let nes = make_nes();
        let emu: &dyn Emulator = &nes;
        // Without a battery-backed cartridge, save_ram should succeed (no-op)
        assert!(emu.save_ram().is_ok());
    }

    #[test]
    fn test_gameboy_trait_save_ram_returns_ok() {
        let gb = make_gameboy();
        let emu: &dyn Emulator = &gb;
        assert!(emu.save_ram().is_ok());
    }

    #[test]
    fn test_nes_trait_joypad_roundtrip() {
        let mut nes = make_nes();
        let emu: &mut dyn Emulator = &mut nes;
        emu.set_joypad_button_states(1, 0b1010_0101);
        assert_eq!(emu.get_joypad_button_states(1), 0b1010_0101);
    }

    // ---------------------------------------------------------------
    // GBA tests — verifies Console::new_gba() and Gba implements Emulator
    // ---------------------------------------------------------------

    fn make_gba() -> Gba {
        Gba::new(make_shared_context())
    }

    #[test]
    fn test_console_new_gba_returns_game_boy_advance_variant() {
        let console = Console::new_gba(make_shared_context());
        assert!(matches!(console, Console::GameBoyAdvance(_)));
    }

    #[test]
    fn test_gba_system_type() {
        let console = Console::new_gba(make_shared_context());
        assert_eq!(console.system_type(), SystemType::Gba);
    }

    #[test]
    fn test_gba_implements_emulator_trait() {
        let gba = make_gba();

        let emu: &dyn Emulator = &gba;
        assert_eq!(emu.system_type(), SystemType::Gba);
        assert_eq!(emu.screen_width(), 240);
        assert_eq!(emu.screen_height(), 160);
        assert!(!emu.is_ready_to_render());
    }

    #[test]
    fn test_console_as_core_delegates_to_gba() {
        let console = Console::new_gba(make_shared_context());

        let emu = console.as_core();
        assert_eq!(emu.system_type(), SystemType::Gba);
        assert_eq!(emu.screen_width(), 240);
        assert_eq!(emu.screen_height(), 160);
    }

    #[test]
    fn test_gba_trait_screen_snapshot_has_correct_size() {
        let gba = make_gba();
        let emu: &dyn Emulator = &gba;
        // 240 × 160 × 3 (RGB888)
        assert_eq!(emu.screen_snapshot().len(), 240 * 160 * 3);
    }

    #[test]
    fn test_gba_trait_target_frame_duration() {
        let gba = make_gba();
        let emu: &dyn Emulator = &gba;
        let dur = emu.target_frame_duration();
        // GBA ≈ 59.73 fps → ~16.7 ms
        assert!(dur.as_millis() > 15 && dur.as_millis() < 20);
    }

    #[test]
    fn test_gba_trait_save_ram_returns_ok() {
        let gba = make_gba();
        let emu: &dyn Emulator = &gba;
        assert!(emu.save_ram().is_ok());
    }

    #[test]
    fn test_gba_allowed_shaders_is_not_empty() {
        let gba = make_gba();
        let shaders = gba.allowed_shaders();
        assert!(!shaders.is_empty());
        assert!(shaders.contains(&"none"));
        assert!(shaders.contains(&"gba-lcd"));
    }

    #[test]
    fn test_gba_get_stereo_sample_returns_none_initially() {
        // Without any APU activity, get_stereo_sample() should return None.
        let mut gba = make_gba();
        gba.set_audio_sample_rate(44_100.0);
        // No ROM loaded → run_tick returns 0 cycles → APU is not clocked → no sample yet.
        assert!(
            !gba.sample_ready(),
            "no sample should be ready on a fresh GBA"
        );
        let stereo = gba.get_stereo_sample();
        assert!(
            stereo.is_none(),
            "get_stereo_sample() must return None when no sample is ready"
        );
    }

    #[test]
    fn test_gba_get_stereo_sample_overrides_emulator_trait() {
        // Verify that Gba correctly overrides the Emulator trait's get_stereo_sample()
        // by using the APU's take_stereo_sample().  We prime the APU directly.
        use crate::gba::apu::Apu;
        // Build an isolated APU, generate a sample with panned audio.
        let mut apu = Apu::new();
        apu.write16(0x0400_0084, 0x0080); // power on
        // SOUNDCNT_H: FIFO A at full vol, RIGHT enable only (bit 8 | bit 2 = 0x0104).
        apu.soundcnt_h = 0x0104;
        apu.push_fifo_a(64);
        apu.fifo_a.advance();
        apu.set_sample_rate(44_100.0);
        let cycles = (16_777_216.0_f32 / 44_100.0) as u32 + 1;
        apu.tick(cycles);
        assert!(apu.sample_ready());
        let (left, right) = apu.take_stereo_sample().unwrap();
        // Right-only FIFO A: right > 0, left == 0.
        assert_eq!(left, 0.0, "left must be 0 for right-only FIFO A");
        assert!(right.abs() > 0.0, "right must have FIFO A audio");
    }

    #[test]
    fn test_default_get_stereo_sample_matches_get_sample_for_nes() {
        // For non-GBA emulators, get_stereo_sample() default should return (s, s)
        // (same as get_sample() on both channels).
        // NES doesn't produce samples without a ROM and isn't trivially ticked, so
        // we verify the default implementation compiles and returns None when
        // no sample is ready.
        let mut nes = crate::nes::console::Nes::new(make_shared_context());
        let stereo = nes.get_stereo_sample();
        assert!(
            stereo.is_none(),
            "no stereo sample should be ready without a ROM"
        );
    }
}

// ---------------------------------------------------------------------------
// Tests for Console::overscan(), Console::cropped_dims(), Console::pixel_aspect()
// and SystemType::windowed_dimensions()
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests_console_abstraction {
    use super::*;
    use crate::nes::console::Config;
    use crate::platform::app_context::AppContext;

    fn make_nes_console_with_overscan(h: u8, v: u8) -> Console {
        let mut config = Config::default();
        config.nes.horizontal_overscan = h;
        config.nes.vertical_overscan = v;
        Console::new_nes(AppContext::new_with_config(config))
    }

    fn make_gb_console() -> Console {
        Console::new_gameboy(AppContext::new_with_config(Config::default()))
    }

    fn make_gba_console() -> Console {
        Console::new_gba(AppContext::new_with_config(Config::default()))
    }

    fn make_app_context_with_overscan(h: u8, v: u8) -> SharedAppContext {
        let mut config = Config::default();
        config.nes.horizontal_overscan = h;
        config.nes.vertical_overscan = v;
        AppContext::new_with_config(config).into_shared()
    }

    // --- Console::overscan() ---

    #[test]
    fn test_nes_overscan_reflects_config() {
        let console = make_nes_console_with_overscan(4, 8);
        assert_eq!(console.overscan(), (4, 8));
    }

    #[test]
    fn test_nes_overscan_zero() {
        let console = make_nes_console_with_overscan(0, 0);
        assert_eq!(console.overscan(), (0, 0));
    }

    #[test]
    fn test_gb_overscan_always_zero() {
        let console = make_gb_console();
        assert_eq!(console.overscan(), (0, 0));
    }

    #[test]
    fn test_gba_overscan_always_zero() {
        let console = make_gba_console();
        assert_eq!(console.overscan(), (0, 0));
    }

    // --- Console::cropped_dims() ---

    #[test]
    fn test_nes_cropped_dims_no_overscan() {
        let console = make_nes_console_with_overscan(0, 0);
        assert_eq!(console.cropped_dims(0, 0), (256, 240));
    }

    #[test]
    fn test_nes_cropped_dims_with_h_overscan() {
        let console = make_nes_console_with_overscan(0, 0);
        assert_eq!(console.cropped_dims(8, 0), (240, 240));
    }

    #[test]
    fn test_nes_cropped_dims_with_v_overscan() {
        let console = make_nes_console_with_overscan(0, 0);
        assert_eq!(console.cropped_dims(0, 8), (256, 224));
    }

    #[test]
    fn test_gb_cropped_dims_ignores_overscan() {
        let console = make_gb_console();
        assert_eq!(console.cropped_dims(8, 8), (160, 144));
    }

    #[test]
    fn test_gba_cropped_dims_ignores_overscan() {
        let console = make_gba_console();
        assert_eq!(console.cropped_dims(8, 8), (240, 160));
    }

    // --- Console::pixel_aspect() ---

    #[test]
    fn test_nes_pixel_aspect_is_eight_sevenths() {
        let console = make_nes_console_with_overscan(0, 0);
        let ratio = console.pixel_aspect();
        assert!((ratio - 8.0 / 7.0).abs() < f32::EPSILON, "got {ratio}");
    }

    #[test]
    fn test_gb_pixel_aspect_is_one() {
        let console = make_gb_console();
        assert_eq!(console.pixel_aspect(), 1.0);
    }

    #[test]
    fn test_gba_pixel_aspect_is_one() {
        let console = make_gba_console();
        assert_eq!(console.pixel_aspect(), 1.0);
    }

    // --- SystemType::windowed_dimensions() ---
    // These tests mirror the moved tests from gl_backend.rs.

    #[test]
    fn test_nes_windowed_dimensions_no_overscan_height_240() {
        let app = make_app_context_with_overscan(0, 0);
        let (w, h) = SystemType::Nes.windowed_dimensions(240, &app);
        assert_eq!(h, 240);
        // 256/240 * 8/7 ≈ 1.2195, width = round(240 * 1.2195) = 293
        assert_eq!(w, 293);
    }

    #[test]
    fn test_nes_windowed_dimensions_no_overscan_height_960() {
        // 960 * (256/240) * (8/7) = 960 * 1.21904... → round = 1170
        let app = make_app_context_with_overscan(0, 0);
        let (w, h) = SystemType::Nes.windowed_dimensions(960, &app);
        assert_eq!(h, 960);
        assert_eq!(w, 1170);
    }

    #[test]
    fn test_nes_windowed_dimensions_h_overscan_narrows_window() {
        // With 8px horizontal overscan the visible area is 240×240 pixels.
        // aspect = (240/240) * (8/7) = 8/7, width = round(240 * 8/7) = 274.
        let app = make_app_context_with_overscan(8, 0);
        let (w, h) = SystemType::Nes.windowed_dimensions(240, &app);
        assert_eq!(h, 240);
        assert_eq!(w, 274);
    }

    #[test]
    fn test_nes_windowed_dimensions_v_overscan_widens_window() {
        // With 8px vertical overscan the visible area is 256×224 pixels.
        // aspect = (256/224) * (8/7) = (8/7)^2 ≈ 1.30612
        // width = round(240 * 1.30612) = 313.
        let app = make_app_context_with_overscan(0, 8);
        let (w, h) = SystemType::Nes.windowed_dimensions(240, &app);
        assert_eq!(h, 240);
        assert_eq!(w, 313);
    }

    #[test]
    fn test_gb_windowed_dimensions_height_144() {
        let app = make_app_context_with_overscan(0, 0);
        let (w, h) = SystemType::GameBoy.windowed_dimensions(144, &app);
        assert_eq!(h, 144);
        assert_eq!(w, 160);
    }

    #[test]
    fn test_gb_windowed_dimensions_height_576() {
        let app = make_app_context_with_overscan(0, 0);
        let (w, h) = SystemType::GameBoy.windowed_dimensions(576, &app);
        assert_eq!(h, 576);
        assert_eq!(w, 640); // 160 × 4
    }

    #[test]
    fn test_gb_windowed_dimensions_height_720() {
        // width = round(720 × 160/144) = round(800.0) = 800.
        let app = make_app_context_with_overscan(0, 0);
        let (w, h) = SystemType::GameBoy.windowed_dimensions(720, &app);
        assert_eq!(h, 720);
        assert_eq!(w, 800);
    }

    #[test]
    fn test_gb_windowed_dimensions_zero_height_clamped() {
        let app = make_app_context_with_overscan(0, 0);
        let (w, h) = SystemType::GameBoy.windowed_dimensions(0, &app);
        assert!(w >= 1);
        assert_eq!(h, 1);
    }

    #[test]
    fn test_gba_windowed_dimensions_height_160() {
        let app = make_app_context_with_overscan(0, 0);
        let (w, h) = SystemType::Gba.windowed_dimensions(160, &app);
        assert_eq!(h, 160);
        assert_eq!(w, 240); // native GBA resolution
    }

    #[test]
    fn test_gba_windowed_dimensions_height_640() {
        // 4× scale: 640 × (240/160) = 960
        let app = make_app_context_with_overscan(0, 0);
        let (w, h) = SystemType::Gba.windowed_dimensions(640, &app);
        assert_eq!(h, 640);
        assert_eq!(w, 960);
    }

    // --- Console::target_frame_duration() ---

    #[test]
    fn test_nes_ntsc_target_frame_duration() {
        let console = make_nes_console_with_overscan(0, 0); // default = NTSC
        let ms = console.target_frame_duration().as_secs_f64() * 1000.0;
        // NTSC: ~60.098 FPS → ~16.64ms per frame
        assert!(
            (16.0..=17.0).contains(&ms),
            "NTSC frame duration should be ~16.6ms, got {ms:.2}ms"
        );
    }

    #[test]
    fn test_nes_pal_target_frame_duration() {
        use crate::nes::console::HardwareModel;
        let mut config = Config::default();
        config.nes.hardware_model = HardwareModel::NesPal;
        let console = Console::new_nes(AppContext::new_with_config(config));
        let ms = console.target_frame_duration().as_secs_f64() * 1000.0;
        // PAL: ~50.007 FPS → ~20.0ms per frame
        assert!(
            (19.5..=20.5).contains(&ms),
            "PAL frame duration should be ~20.0ms, got {ms:.2}ms"
        );
    }

    #[test]
    fn test_gb_target_frame_duration() {
        let console = make_gb_console();
        let ms = console.target_frame_duration().as_secs_f64() * 1000.0;
        // DMG GB: 4,194,304 Hz / 70,224 cycles ≈ 59.73 FPS → ~16.74ms
        assert!(
            (16.5..=17.0).contains(&ms),
            "GB frame duration should be ~16.74ms, got {ms:.2}ms"
        );
    }
}
