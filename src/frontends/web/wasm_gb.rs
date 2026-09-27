use crate::gb::console::gameboy::GameBoy;
use crate::platform::app_context::{AppContext, SharedAppContext};
use crate::web_console::{WebConsole, opaque_black_rgba, web_console_bindings};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

/// Provides a minimal WASM bridge for running the Game Boy emulator in the browser.
#[wasm_bindgen]
pub struct WasmGb {
    web: WebConsole<GameBoy>,
}

web_console_bindings!(WasmGb);

impl Default for WasmGb {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl WasmGb {
    fn rgb_to_rgba(rgb: &[u8]) -> Vec<u8> {
        rgb.as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 0xFF])
            .collect()
    }

    fn gb(&self) -> &GameBoy {
        self.web.core()
    }

    fn gb_mut(&mut self) -> &mut GameBoy {
        self.web.core_mut()
    }

    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmGb {
        console_error_panic_hook::set_once();
        let app_context: SharedAppContext = Rc::new(RefCell::new(AppContext::new_with_config(
            Default::default(),
        )));
        WasmGb {
            web: WebConsole::new(GameBoy::new(app_context)),
        }
    }

    /// Load a `.gb` ROM from raw bytes.
    #[wasm_bindgen]
    pub fn load_rom(&mut self, rom: &[u8], rom_name: &str) -> Result<(), JsValue> {
        self.web
            .load_rom(rom, rom_name)
            .map_err(|err| JsValue::from_str(&err))?;
        web_sys::console::log_1(&JsValue::from_str("GB ROM loaded successfully"));
        Ok(())
    }

    /// The Colors button's label: the colour correction's state in the words
    /// of its corner message.
    #[wasm_bindgen]
    pub fn color_label(&self) -> String {
        self.gb().color_correction_label()
    }

    /// The palette in use, as the Palette button names it ("Palette: <name>",
    /// the toast's words), or `""` where F8 cycles no palette.
    #[wasm_bindgen]
    pub fn palette_label(&self) -> String {
        self.gb().palette_label().unwrap_or_default()
    }

    /// Test access to the console, e.g. to choose the hardware.
    #[cfg(all(test, target_arch = "wasm32"))]
    pub(crate) fn game_boy_mut(&mut self) -> &mut GameBoy {
        self.gb_mut()
    }

    /// Called when a game starts, with whether the Game Boy LCD filter is on.
    #[wasm_bindgen]
    pub fn start_lcd_filter(&mut self, active: bool) {
        self.gb_mut().start_lcd_filter(active);
    }

    /// Called when the Game Boy LCD filter is switched on or off.
    #[wasm_bindgen]
    pub fn set_lcd_filter_active(&mut self, active: bool) {
        self.gb_mut().set_lcd_filter_active(active);
    }

    /// The LCD filter's palette texture as 2x1 RGBA: background, foreground.
    #[wasm_bindgen]
    pub fn lcd_filter_palette_rgba(&self) -> Vec<u8> {
        let [(br, bg, bb), (fr, fg, fb)] = self.gb().lcd_filter_colors();
        vec![br, bg, bb, 0xFF, fr, fg, fb, 0xFF]
    }

    /// Step the emulator until a full frame is ready and return the pixel buffer (RGBA8888).
    ///
    /// Returns a `Uint8Array` of `160 × 144 × 4` bytes.
    /// When no ROM is loaded, returns an opaque black frame.
    #[wasm_bindgen]
    pub fn render_frame_rgba(&mut self) -> Vec<u8> {
        if !self.web.rom_loaded() {
            return opaque_black_rgba(GameBoy::SCREEN_WIDTH, GameBoy::SCREEN_HEIGHT);
        }
        self.web.run_until_frame_ready();
        let rgb = self.gb().screen_snapshot();
        Self::rgb_to_rgba(&rgb)
    }

    /// Returns `true` when a game is loaded and shown in colour (a Game Boy
    /// Color game, or a black-and-white game the Game Boy Color colourises).
    #[wasm_bindgen]
    pub fn is_color(&self) -> bool {
        self.web.rom_loaded() && self.gb().is_cgb_mode()
    }

    /// "Game Boy games run on": whether original Game Boy games run on the
    /// Game Boy Color (`true`) or the Game Boy (`false`, auto-detect, so a
    /// Game Boy Color game still runs on the Game Boy Color). Applies from
    /// the next load or reset; the running game carries on unchanged.
    #[wasm_bindgen]
    pub fn set_original_games_on_color(&mut self, on: bool) {
        self.gb()
            .app_context()
            .borrow_mut()
            .config_mut()
            .gb
            .hardware = on.then_some(crate::gb::model::GbHardware::Cgb);
    }

    /// `true` when an original Game Boy game is loaded, on either console.
    #[wasm_bindgen]
    pub fn is_original_game(&self) -> bool {
        self.web.rom_loaded() && self.gb().is_original_game()
    }

    /// Turn the Game Boy Color LCD colour correction on or off.
    ///
    /// Takes effect from the next rendered frame, and stays set for any game
    /// later loaded into this instance.
    #[wasm_bindgen]
    pub fn set_cgb_color_correction(&mut self, enabled: bool) {
        self.gb_mut()
            .app_context()
            .borrow_mut()
            .config_mut()
            .gb
            .cgb_color_correction = enabled;
    }

    /// Whether the Game Boy Color LCD colour correction is on (set by the
    /// Colors button or switched with F8).
    #[wasm_bindgen]
    pub fn cgb_color_correction(&self) -> bool {
        self.gb().cgb_color_correction()
    }

    /// Returns the display width in pixels (always 160 for Game Boy).
    #[wasm_bindgen]
    pub fn screen_width(&self) -> u32 {
        GameBoy::SCREEN_WIDTH
    }

    /// Returns the display height in pixels (always 144 for Game Boy).
    #[wasm_bindgen]
    pub fn screen_height(&self) -> u32 {
        GameBoy::SCREEN_HEIGHT
    }

    /// Returns the nominal Game Boy refresh rate in Hz.
    ///
    /// DMG: 4,194,304 Hz / 70,224 cycles per frame ≈ 59.7275 Hz.
    #[wasm_bindgen]
    pub fn frame_rate_hz(&self) -> f64 {
        4_194_304.0 / 70_224.0
    }

    /// Set button state for the Game Boy joypad.
    ///
    /// Uses NES-convention IDs: A=0, B=1, Select=2, Start=3, Up=4, Down=5, Left=6, Right=7.
    /// Only controller port 1 is used (Game Boy has a single joypad).
    #[wasm_bindgen]
    pub fn set_button(&mut self, controller: u8, button: u8, pressed: bool) {
        if controller == 1 {
            self.gb_mut().set_button(button, pressed);
        }
    }

    /// Reset the emulator; an original Game Boy game starts over on the
    /// console chosen with `set_original_games_on_color` if that changed.
    #[wasm_bindgen]
    pub fn reset(&mut self, soft_reset: bool) {
        self.web.reset(soft_reset);
    }

    /// Serialize the current emulator state to bytes.
    #[wasm_bindgen]
    pub fn save_state_bytes(&self) -> Vec<u8> {
        self.gb().save_state_bytes().unwrap_or_default()
    }

    /// Restore emulator state from previously serialized bytes, on the
    /// console it was saved on.
    #[wasm_bindgen]
    pub fn load_state_bytes(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        self.gb_mut()
            .load_state_bytes_as_saved(bytes)
            .map_err(|e| JsValue::from_str(&e))
    }
}
