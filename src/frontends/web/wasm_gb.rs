use crate::gb::console::gameboy::GameBoy;
use crate::platform::app_context::{AppContext, SharedAppContext};
use crate::platform::frontend_toasts::cartridge_load_toast_message;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

/// Provides a minimal WASM bridge for running the Game Boy emulator in the browser.
#[wasm_bindgen]
pub struct WasmGb {
    gb: GameBoy,
    audio_muted: bool,
    rom_loaded: bool,
    pending_toasts: Vec<String>,
}

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

    fn opaque_black_rgba_frame() -> Vec<u8> {
        let pixel_count = (GameBoy::SCREEN_WIDTH * GameBoy::SCREEN_HEIGHT) as usize;
        let mut rgba = vec![0u8; pixel_count * 4];
        for alpha in rgba.iter_mut().skip(3).step_by(4) {
            *alpha = 0xFF;
        }
        rgba
    }

    fn run_until_frame_ready(&mut self) {
        while !self.gb.is_frame_ready() {
            self.gb.run_tick();
        }
        self.gb.clear_frame_ready();
    }

    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmGb {
        console_error_panic_hook::set_once();
        let app_context: SharedAppContext = Rc::new(RefCell::new(AppContext::new_with_config(
            Default::default(),
        )));
        WasmGb {
            gb: GameBoy::new(app_context),
            audio_muted: false,
            rom_loaded: false,
            pending_toasts: Vec::new(),
        }
    }

    /// Load a `.gb` ROM from raw bytes.
    #[wasm_bindgen]
    pub fn load_rom(&mut self, rom: &[u8], rom_name: &str) -> Result<(), JsValue> {
        self.rom_loaded = false;
        match self.gb.load_rom(rom, rom_name) {
            Ok(()) => {
                self.rom_loaded = true;
                self.gb.set_audio_sample_rate(44100.0);
                self.pending_toasts
                    .push(cartridge_load_toast_message(rom_name, true));
                web_sys::console::log_1(&JsValue::from_str("GB ROM loaded successfully"));
                Ok(())
            }
            Err(err) => {
                self.pending_toasts
                    .push(cartridge_load_toast_message(rom_name, false));
                Err(JsValue::from_str(&err))
            }
        }
    }

    /// Drain any pending toast messages.
    #[wasm_bindgen]
    pub fn drain_toasts(&mut self) -> Vec<JsValue> {
        self.pending_toasts.drain(..).map(JsValue::from).collect()
    }

    /// F8: cycles an original Game Boy game's palette and queues the toast:
    /// the shade palette on Game Boy hardware, the colourisation on Game Boy
    /// Color hardware. Returns the new palette's name, or `""` when no
    /// original Game Boy game is running (nothing changes, no toast).
    #[wasm_bindgen]
    pub fn cycle_palette(&mut self) -> String {
        if let Some(palette) = self.gb.cycle_palette() {
            self.pending_toasts
                .push(crate::gb::ppu::dmg_palette::palette_toast_message(palette));
            return palette.display_name().to_string();
        }
        match self.gb.cycle_gbc_palette() {
            Some(toast) => {
                self.pending_toasts.push(toast);
                self.gb.gbc_palette().display_name().to_string()
            }
            None => String::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn game_boy_mut(&mut self) -> &mut GameBoy {
        &mut self.gb
    }

    /// Called when a game starts, with whether the Game Boy LCD filter is on.
    #[wasm_bindgen]
    pub fn start_lcd_filter(&mut self, active: bool) {
        self.gb.start_lcd_filter(active);
    }

    /// Called when the Game Boy LCD filter is switched on or off.
    #[wasm_bindgen]
    pub fn set_lcd_filter_active(&mut self, active: bool) {
        self.gb.set_lcd_filter_active(active);
    }

    /// The LCD filter's palette texture as 2x1 RGBA: background, foreground.
    #[wasm_bindgen]
    pub fn lcd_filter_palette_rgba(&self) -> Vec<u8> {
        let [(br, bg, bb), (fr, fg, fb)] = self.gb.lcd_filter_colors();
        vec![br, bg, bb, 0xFF, fr, fg, fb, 0xFF]
    }

    /// Step the emulator until a full frame is ready and return the pixel buffer (RGBA8888).
    ///
    /// Returns a `Uint8Array` of `160 × 144 × 4` bytes.
    /// When no ROM is loaded, returns an opaque black frame.
    #[wasm_bindgen]
    pub fn render_frame_rgba(&mut self) -> Vec<u8> {
        if !self.rom_loaded {
            return Self::opaque_black_rgba_frame();
        }
        self.run_until_frame_ready();
        let rgb = self.gb.screen_snapshot();
        Self::rgb_to_rgba(&rgb)
    }

    /// Returns `true` when a game is loaded and shown in colour (a Game Boy
    /// Color game, or a black-and-white game the Game Boy Color colourises).
    #[wasm_bindgen]
    pub fn is_color(&self) -> bool {
        self.rom_loaded && self.gb.is_cgb_mode()
    }

    /// Turn the Game Boy Color LCD colour correction on or off.
    ///
    /// Takes effect from the next rendered frame, and stays set for any game
    /// later loaded into this instance.
    #[wasm_bindgen]
    pub fn set_cgb_color_correction(&mut self, enabled: bool) {
        self.gb
            .app_context()
            .borrow_mut()
            .config_mut()
            .gb
            .cgb_color_correction = enabled;
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

    fn drain_audio_buffer(&mut self) {
        while self.gb.get_sample().is_some() {}
    }

    /// Collect all pending audio samples from the APU.
    ///
    /// Returns a `Float32Array`. Call after each `render_frame_rgba`.
    #[wasm_bindgen]
    pub fn get_audio_samples(&mut self) -> Vec<f32> {
        if self.audio_muted {
            self.drain_audio_buffer();
            return Vec::new();
        }
        let mut samples = Vec::new();
        while let Some(s) = self.gb.get_sample() {
            samples.push(s);
        }
        samples
    }

    /// Set the emulator audio output sample rate in Hz.
    #[wasm_bindgen]
    pub fn set_audio_sample_rate(&mut self, sample_rate: f32) {
        self.gb.set_audio_sample_rate(sample_rate);
    }

    /// Set audio mute state.
    #[wasm_bindgen]
    pub fn set_audio_muted(&mut self, muted: bool) {
        self.audio_muted = muted;
        if muted {
            self.drain_audio_buffer();
        }
    }

    /// Returns `true` if audio is currently muted.
    #[wasm_bindgen]
    pub fn is_audio_muted(&self) -> bool {
        self.audio_muted
    }

    /// Set button state for the Game Boy joypad.
    ///
    /// Uses NES-convention IDs: A=0, B=1, Select=2, Start=3, Up=4, Down=5, Left=6, Right=7.
    /// Only controller port 1 is used (Game Boy has a single joypad).
    #[wasm_bindgen]
    pub fn set_button(&mut self, controller: u8, button: u8, pressed: bool) {
        if controller == 1 {
            self.gb.set_button(button, pressed);
        }
    }

    /// Reset the emulator.
    #[wasm_bindgen]
    pub fn reset(&mut self, soft_reset: bool) {
        self.gb.reset(soft_reset);
    }

    /// Serialize the current emulator state to bytes.
    #[wasm_bindgen]
    pub fn save_state_bytes(&self) -> Vec<u8> {
        self.gb.save_state_bytes().unwrap_or_default()
    }

    /// Restore emulator state from previously serialized bytes.
    #[wasm_bindgen]
    pub fn load_state_bytes(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        self.gb
            .load_state_bytes(bytes)
            .map_err(|e| JsValue::from_str(&e))
    }
}
