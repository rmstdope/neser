use crate::cartridge::Cartridge;
use crate::console::{Config, Nes, SaveState};
use crate::input::Button;
use wasm_bindgen::prelude::*;

/// Provides a minimal WASM bridge for running the emulator in the browser.
///
/// Note: NTSC timing is hardcoded for the MVP; PAL titles will run at the wrong speed.
#[wasm_bindgen]
pub struct WasmNes {
    nes: Nes,
    audio_muted: bool,
}

impl Default for WasmNes {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl WasmNes {
    fn drain_audio_samples(&mut self) {
        while self.nes.get_sample().is_some() {}
    }

    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmNes {
        console_error_panic_hook::set_once();
        WasmNes {
            nes: Nes::new(Config::default()),
            audio_muted: false,
        }
    }

    /// Load a ROM from raw bytes.
    #[wasm_bindgen]
    pub fn load_rom(&mut self, rom: &[u8]) -> Result<(), JsValue> {
        self.nes = Nes::new(Config::default());
        let cart = Cartridge::new(rom).map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.nes.insert_cartridge(cart);
        self.nes.reset(false);
        Ok(())
    }

    /// Reset the emulator without ejecting the cartridge.
    #[wasm_bindgen]
    pub fn reset(&mut self) {
        self.nes.reset(true);
    }

    /// Step the emulator until a full frame is ready and return the pixel buffer (RGB888).
    ///
    /// Returns a Uint8Array with length 256*240*3.
    #[wasm_bindgen]
    pub fn render_frame(&mut self) -> Vec<u8> {
        // NOTE: MVP implementation runs a synchronous loop until a frame is ready.
        // For browser responsiveness, this could be broken into smaller chunks via
        // async/yield in a future enhancement. See web/README.md for notes about
        // potential main-thread blocking with heavy frames.
        while !self.nes.is_ready_to_render() {
            self.nes.run_cpu_tick();
        }
        self.nes.clear_ready_to_render();
        self.nes.get_screen_buffer().snapshot()
    }

    /// Step the emulator until a full frame is ready and return the pixel buffer (RGBA8888).
    ///
    /// Returns a Uint8Array with length 256*240*4 (alpha set to 0xFF).
    #[wasm_bindgen]
    pub fn render_frame_rgba(&mut self) -> Vec<u8> {
        while !self.nes.is_ready_to_render() {
            self.nes.run_cpu_tick();
        }
        self.nes.clear_ready_to_render();
        let rgb = self.nes.get_screen_buffer().snapshot();
        let pixel_count = rgb.len() / 3;
        let mut rgba = vec![0u8; pixel_count * 4];
        for i in 0..pixel_count {
            let rgb_idx = i * 3;
            let rgba_idx = i * 4;
            rgba[rgba_idx] = rgb[rgb_idx];
            rgba[rgba_idx + 1] = rgb[rgb_idx + 1];
            rgba[rgba_idx + 2] = rgb[rgb_idx + 2];
            rgba[rgba_idx + 3] = 0xFF;
        }
        rgba
    }

    /// Set button state for a controller.
    ///
    /// # Arguments
    /// * `controller` - Controller number (1 or 2)
    /// * `button` - Button number (0=A, 1=B, 2=Select, 3=Start, 4=Up, 5=Down, 6=Left, 7=Right)
    /// * `pressed` - true if pressed, false if released
    #[wasm_bindgen]
    pub fn set_button(&mut self, controller: u8, button: u8, pressed: bool) {
        let nes_button = match button {
            0 => Button::A,
            1 => Button::B,
            2 => Button::Select,
            3 => Button::Start,
            4 => Button::Up,
            5 => Button::Down,
            6 => Button::Left,
            7 => Button::Right,
            _ => return, // Invalid button, ignore
        };
        self.nes.set_button(controller, nes_button, pressed);
    }

    /// Returns `true` if paddle 1 is enabled.
    #[wasm_bindgen]
    pub fn paddle1_enabled(&self) -> bool {
        self.nes.paddle_port() == Some(1)
    }

    /// Returns the controller port number that has a paddle, or null if no paddle.
    ///
    /// # Returns
    /// * `Some(1)` if paddle on port 1
    /// * `Some(2)` if paddle on port 2
    /// * `None` if no paddle connected
    #[wasm_bindgen]
    pub fn paddle_port(&self) -> Option<u8> {
        self.nes.paddle_port()
    }

    /// Set the current position of paddle 1 (0..=255).
    #[wasm_bindgen]
    pub fn set_paddle1_position(&mut self, position: u8) {
        self.nes.set_paddle1_position(position);
    }

    /// Set the trigger button state for paddle 1.
    #[wasm_bindgen]
    pub fn set_paddle1_trigger(&mut self, pressed: bool) {
        self.nes.set_paddle1_trigger(pressed);
    }

    /// Set the current position of a paddle controller on a specific port.
    ///
    /// # Arguments
    /// * `port` - Controller port (1 or 2)
    /// * `position` - The paddle position value (0..=255)
    #[wasm_bindgen]
    pub fn set_paddle_position(&mut self, port: u8, position: u8) {
        self.nes.set_paddle_position(port, position);
    }

    /// Set the trigger button state for a paddle controller on a specific port.
    ///
    /// # Arguments
    /// * `port` - Controller port (1 or 2)
    /// * `pressed` - true if pressed, false if released
    #[wasm_bindgen]
    pub fn set_paddle_trigger(&mut self, port: u8, pressed: bool) {
        self.nes.set_paddle_trigger(port, pressed);
    }

    /// Get all available audio samples from the emulator.
    ///
    /// Returns a Float32Array containing all pending audio samples.
    /// Each sample is typically in the range 0.0 to ~1.177. The base APU mixer
    /// (pulse + TND) produces values up to roughly 0.966, and expansion audio
    /// from certain mappers (e.g., VRC6, MMC5, Namco 163) can increase this
    /// further. A conservative maximum of 1.177 is used for normalization.
    /// Call this after each frame to retrieve accumulated audio samples.
    #[wasm_bindgen]
    pub fn get_audio_samples(&mut self) -> Vec<f32> {
        if self.audio_muted {
            self.drain_audio_samples();
            return Vec::new();
        }
        let mut samples = Vec::new();
        while let Some(sample) = self.nes.get_sample() {
            samples.push(sample);
        }
        samples
    }

    /// Serialize the current emulator state to JSON bytes.
    #[wasm_bindgen]
    pub fn save_state_bytes(&self) -> Vec<u8> {
        self.nes.save_state().to_bytes().unwrap_or_default()
    }

    /// Load a previously saved emulator state from JSON bytes.
    #[wasm_bindgen]
    pub fn load_state_bytes(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        let state = SaveState::from_bytes(bytes).map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.nes
            .load_state(&state)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Set audio mute state.
    #[wasm_bindgen]
    pub fn set_audio_muted(&mut self, muted: bool) {
        self.audio_muted = muted;
        if muted {
            self.drain_audio_samples();
        }
    }

    /// Returns true if audio is muted.
    #[wasm_bindgen]
    pub fn is_audio_muted(&self) -> bool {
        self.audio_muted
    }

    #[cfg(test)]
    #[wasm_bindgen]
    pub fn push_audio_sample_for_test(&mut self, sample: f32) {
        self.nes.apu.borrow_mut().push_sample_for_test(sample);
    }
}
