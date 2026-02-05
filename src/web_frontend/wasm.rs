use crate::cartridge::Cartridge;
use crate::console::{Config, Nes, SaveState};
use crate::input::{Button, ControllerType};
use wasm_bindgen::prelude::*;

/// Provides a minimal WASM bridge for running the emulator in the browser.
///
/// Note: NTSC timing is hardcoded for the MVP; PAL titles will run at the wrong speed.
#[wasm_bindgen]
pub struct WasmNes {
    nes: Nes,
    audio_muted: bool,
    rom_loaded: bool,
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
            rom_loaded: false,
        }
    }

    /// Load a ROM from raw bytes.
    #[wasm_bindgen]
    pub fn load_rom(&mut self, rom: &[u8]) -> Result<(), JsValue> {
        self.nes = Nes::new(Config::default());
        self.rom_loaded = false;
        let cart = Cartridge::new(rom).map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.nes.insert_cartridge(cart);
        self.nes.reset(false);
        self.rom_loaded = true;
        web_sys::console::log_1(&JsValue::from_str("ROM loaded successfully"));
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
        if !self.rom_loaded {
            return vec![0u8; 256 * 240 * 3];
        }
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
        if !self.rom_loaded {
            let pixel_count = 256 * 240;
            let mut rgba = vec![0u8; pixel_count * 4];
            for alpha in rgba.iter_mut().skip(3).step_by(4) {
                *alpha = 0xFF;
            }
            return rgba;
        }
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

    /// Set the controller type for a port.
    ///
    /// # Arguments
    /// * `port` - Controller port (1 or 2)
    /// * `controller_type` - "joypad" or "arkanoid"
    #[wasm_bindgen]
    pub fn set_controller_type(&mut self, port: u8, controller_type: &str) -> Result<(), JsValue> {
        let controller_type = ControllerType::parse(controller_type)
            .ok_or_else(|| JsValue::from_str("invalid controller type"))?;
        self.nes
            .bus
            .borrow_mut()
            .set_controller_type(port, controller_type);
        Ok(())
    }

    /// Check if mouse-emulated controller input is enabled on a port.
    /// Returns true if a mouse-emulated controller is active on the specified port.
    /// This is used by the JavaScript frontend to determine whether to suppress joypad input for that port.
    #[wasm_bindgen]
    pub fn is_mouse_emulated_controller(&self, port: u8) -> bool {
        self.nes.controller_input_type(port) == Some(crate::input::ControllerInput::Mouse)
    }

    /// Set the mouse X position for any mouse-emulated controller.
    ///
    /// # Arguments
    /// * `position` - The mouse-emulated controller position value (0..=255)
    #[wasm_bindgen]
    pub fn set_mouse_x_position(&mut self, position: u8) {
        self.nes.set_mouse_x_position(position);
    }

    /// Set the mouse Y position for any mouse-emulated controller.
    ///
    /// # Arguments
    /// * `position` - The mouse-emulated controller position value (0..=239)
    #[wasm_bindgen]
    pub fn set_mouse_y_position(&mut self, position: u8) {
        self.nes.set_mouse_y_position(position);
    }

    /// Set the mouse left button state for any mouse-emulated controller.
    ///
    /// # Arguments
    /// * `pressed` - true if pressed, false if released
    #[wasm_bindgen]
    pub fn set_mouse_left_button(&mut self, pressed: bool) {
        self.nes.set_mouse_left_button(pressed);
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
