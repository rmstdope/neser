//! Hardware-agnostic emulator interface.
//!
//! The [`Console`] enum wraps system-specific emulators (currently only NES)
//! and provides a common interface that frontends can program against.
//! NES-specific features (debugging, PPU viewer, etc.) are accessed by
//! matching on the [`Console::Nes`] variant directly.

use crate::gb::GameBoy;
use crate::nes::console::Nes;
use crate::platform::app_context::{IntoSharedAppContext, SharedAppContext};

/// Identifies which emulated system a [`Console`] instance is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemType {
    Nes,
    GameBoy,
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

    /// Which system this console is emulating.
    pub fn system_type(&self) -> SystemType {
        match self {
            Console::Nes(_) => SystemType::Nes,
            Console::GameBoy(_) => SystemType::GameBoy,
        }
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
        match self {
            Console::Nes(nes) => nes.load_rom(bytes, name),
            Console::GameBoy(gb) => gb.load_rom(bytes, name),
        }
    }

    /// Execute one CPU tick (instruction) and advance all subsystems.
    ///
    /// Returns the number of CPU cycles consumed.
    pub fn run_tick(&mut self) -> u8 {
        match self {
            Console::Nes(nes) => nes.run_cpu_tick(),
            Console::GameBoy(gb) => gb.run_tick(),
        }
    }

    /// Returns `true` when a complete frame has been rendered and is ready
    /// for display.
    pub fn is_ready_to_render(&self) -> bool {
        match self {
            Console::Nes(nes) => nes.is_ready_to_render(),
            Console::GameBoy(gb) => gb.is_frame_ready(),
        }
    }

    /// Clear the frame-ready flag after the frontend has consumed the frame.
    pub fn clear_ready_to_render(&mut self) {
        match self {
            Console::Nes(nes) => nes.clear_ready_to_render(),
            Console::GameBoy(gb) => gb.clear_frame_ready(),
        }
    }

    /// Width of the emulated display in pixels.
    pub fn screen_width(&self) -> u32 {
        match self {
            Console::Nes(_) => Nes::SCREEN_WIDTH,
            Console::GameBoy(_) => GameBoy::SCREEN_WIDTH,
        }
    }

    /// Height of the emulated display in pixels.
    pub fn screen_height(&self) -> u32 {
        match self {
            Console::Nes(_) => Nes::SCREEN_HEIGHT,
            Console::GameBoy(_) => GameBoy::SCREEN_HEIGHT,
        }
    }

    /// Returns a snapshot of the current frame as RGB888 bytes.
    ///
    /// The returned buffer has `screen_width() * screen_height() * 3` bytes,
    /// ordered row-major with 3 bytes per pixel (R, G, B).
    pub fn screen_snapshot(&self) -> Vec<u8> {
        match self {
            Console::Nes(nes) => nes.get_screen_buffer().snapshot(),
            Console::GameBoy(gb) => gb.screen_snapshot(),
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
            Console::GameBoy(gb) => gb.cropped_screen_snapshot(),
        }
    }

    /// CRC32 of the current screen buffer (for autorun verification).
    pub fn screen_crc32(&self) -> u32 {
        match self {
            Console::Nes(nes) => nes.get_screen_buffer().crc32(),
            Console::GameBoy(gb) => gb.screen_crc32(),
        }
    }

    /// Returns `true` when an audio sample is ready for retrieval.
    pub fn sample_ready(&self) -> bool {
        match self {
            Console::Nes(nes) => nes.sample_ready(),
            Console::GameBoy(gb) => gb.sample_ready(),
        }
    }

    /// Retrieve the next audio sample, if one is ready.
    ///
    /// Returns a sample in the range `0.0..=1.0`, or `None` if no sample
    /// is pending.
    pub fn get_sample(&mut self) -> Option<f32> {
        match self {
            Console::Nes(nes) => nes.get_sample(),
            Console::GameBoy(gb) => gb.get_sample(),
        }
    }

    /// Set a button state on a controller port.
    ///
    /// `button_id` is system-specific: for NES, it maps to [`crate::nes::input::Button`]
    /// discriminant values (A=0, B=1, Select=2, Start=3, Up=4, Down=5, Left=6, Right=7).
    /// For Game Boy, only `port == 0` is meaningful; calls for other ports are ignored.
    pub fn set_button(&mut self, port: u8, button_id: u8, pressed: bool) {
        match self {
            Console::Nes(nes) => {
                if !nes.set_button_by_id(port, button_id, pressed) {
                    #[cfg(debug_assertions)]
                    eprintln!("warning: invalid NES button_id: {button_id}");
                }
            }
            Console::GameBoy(gb) => {
                if port == 0 {
                    gb.set_button(button_id, pressed);
                }
            }
        }
    }

    /// Set all button states from a bitmask (for autorun playback).
    ///
    /// Each bit corresponds to a button by its system-specific ID.
    /// For Game Boy, only `port == 0` is applied; other ports are ignored.
    pub fn set_joypad_button_states(&mut self, port: u8, state: u8) {
        match self {
            Console::Nes(nes) => nes.set_joypad_button_states(port, state),
            Console::GameBoy(gb) => {
                if port == 0 {
                    gb.set_joypad_button_states(state);
                }
            }
        }
    }

    /// Get all button states as a bitmask (for autorun recording).
    ///
    /// For Game Boy, only `port == 0` returns button states; other ports return 0.
    pub fn get_joypad_button_states(&self, port: u8) -> u8 {
        match self {
            Console::Nes(nes) => nes.get_joypad_button_states(port),
            Console::GameBoy(gb) => {
                if port == 0 {
                    gb.get_joypad_button_states()
                } else {
                    0
                }
            }
        }
    }

    /// Serialize the complete emulator state to bytes.
    pub fn save_state_bytes(&self) -> Result<Vec<u8>, String> {
        match self {
            Console::Nes(nes) => nes.save_state_bytes(),
            Console::GameBoy(gb) => gb.save_state_bytes(),
        }
    }

    /// Restore emulator state from previously serialized bytes.
    pub fn load_state_bytes(&mut self, data: &[u8]) -> Result<(), String> {
        match self {
            Console::Nes(nes) => nes.load_state_bytes(data),
            Console::GameBoy(gb) => gb.load_state_bytes(data),
        }
    }

    /// Reset the emulator.
    ///
    /// `soft_reset` = true simulates pressing the reset button.
    /// `soft_reset` = false simulates a power cycle.
    pub fn reset(&mut self, soft_reset: bool) {
        match self {
            Console::Nes(nes) => nes.reset(soft_reset),
            Console::GameBoy(gb) => gb.reset(soft_reset),
        }
    }

    /// Access the shared application context (config, ROM database, toasts).
    pub fn app_context(&self) -> &SharedAppContext {
        match self {
            Console::Nes(nes) => nes.app_context(),
            Console::GameBoy(gb) => gb.app_context(),
        }
    }

    /// Save battery-backed RAM to disk (if applicable).
    pub fn save_ram(&self) -> Result<(), String> {
        match self {
            Console::Nes(nes) => nes.save_ram().map_err(|e| e.to_string()),
            Console::GameBoy(_) => Ok(()),
        }
    }

    /// Set the audio output sample rate (Hz) for the emulator's APU.
    pub fn set_audio_sample_rate(&mut self, rate: f32) {
        match self {
            Console::Nes(nes) => nes.set_audio_sample_rate(rate),
            Console::GameBoy(gb) => gb.set_audio_sample_rate(rate),
        }
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
            Console::GameBoy(_) => (0, 0),
        }
    }

    /// Visible screen dimensions in pixels with overscan removed.
    ///
    /// For NES, `h_overscan` columns are removed from each side and `v_overscan`
    /// rows from top and bottom.  For Game Boy, overscan parameters are ignored
    /// and the native 160×144 resolution is always returned.
    pub fn cropped_dims(&self, h_overscan: u32, v_overscan: u32) -> (u32, u32) {
        match self {
            Console::Nes(_) => (
                self.screen_width().saturating_sub(2 * h_overscan).max(1),
                self.screen_height().saturating_sub(2 * v_overscan).max(1),
            ),
            Console::GameBoy(_) => (self.screen_width(), self.screen_height()),
        }
    }

    /// Pixel aspect ratio correction factor for the current system.
    ///
    /// NES pixels are not square: the NTSC hardware maps 256 pixels across the
    /// same horizontal extent as approximately 280 square pixels (8:7 ratio).
    /// Game Boy pixels are square, so the correction factor is 1.0.
    pub fn pixel_aspect(&self) -> f32 {
        match self {
            Console::Nes(_) => 8.0 / 7.0,
            Console::GameBoy(_) => 1.0,
        }
    }

    /// Target wall-clock duration between rendered frames for this system.
    ///
    /// Frontends use this to pace emulation correctly regardless of display
    /// refresh rate.  The NES value is derived from the hardware timing mode
    /// (NTSC ≈ 60.10 fps, PAL ≈ 50.01 fps).  The DMG Game Boy always runs at
    /// 4,194,304 Hz / 70,224 cycles per frame ≈ 59.73 fps.
    pub fn target_frame_duration(&self) -> std::time::Duration {
        match self {
            Console::GameBoy(_) => {
                // DMG: 4,194,304 Hz clock / 70,224 cycles per frame ≈ 59.7275 fps
                std::time::Duration::from_secs_f64(70_224.0 / 4_194_304.0)
            }
            Console::Nes(_) => {
                let hz = self
                    .app_context()
                    .borrow()
                    .config()
                    .nes
                    .hardware_model
                    .timing_mode()
                    .frame_rate_hz();
                std::time::Duration::from_secs_f64(1.0 / hz)
            }
        }
    }
}

impl SystemType {
    /// Computes windowed-mode dimensions that preserve the system's correct aspect ratio.
    ///
    /// For NES, overscan is read from `app_context` and the NTSC 8:7 pixel aspect
    /// ratio is applied.  For Game Boy, square pixels (1:1) are assumed and there
    /// is no overscan.
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
    // Covers the core subset of Console's public methods. When adding
    // a new generic method to Console, also add it here so that
    // GameBoyStub must implement it, catching gaps at compile time.
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
}
