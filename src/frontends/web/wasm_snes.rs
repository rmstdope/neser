use crate::platform::app_context::{AppContext, SharedAppContext};
use crate::platform::emulator::Emulator;
use crate::platform::frontend_toasts::cartridge_load_toast_message;
use crate::snes::console::Snes;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

/// Provides a minimal WASM bridge for running the Super Nintendo emulator in the browser.
#[wasm_bindgen]
pub struct WasmSnes {
    snes: Snes,
    audio_muted: bool,
    rom_loaded: bool,
    pending_toasts: Vec<String>,
}

impl Default for WasmSnes {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl WasmSnes {
    fn physical_port(port: u8) -> Option<u8> {
        match port {
            1 => Some(0),
            2 => Some(1),
            _ => None,
        }
    }

    fn rgba_frame_from_snapshot(
        rgb: &[u8],
        src_width: u32,
        src_height: u32,
        dst_width: u32,
        dst_height: u32,
    ) -> Vec<u8> {
        let mut rgba = vec![0u8; (dst_width * dst_height * 4) as usize];
        for alpha in rgba.iter_mut().skip(3).step_by(4) {
            *alpha = 0xFF;
        }

        let copy_width = src_width.min(dst_width) as usize;
        let copy_height = src_height.min(dst_height) as usize;
        let src_width = src_width as usize;
        let dst_width = dst_width as usize;

        for y in 0..copy_height {
            for x in 0..copy_width {
                let src_index = (y * src_width + x) * 3;
                let dst_index = (y * dst_width + x) * 4;
                rgba[dst_index..dst_index + 3].copy_from_slice(&rgb[src_index..src_index + 3]);
            }
        }

        rgba
    }

    fn opaque_black_rgba_frame() -> Vec<u8> {
        let pixel_count = (Snes::SCREEN_WIDTH * Snes::SCREEN_HEIGHT) as usize;
        let mut rgba = vec![0u8; pixel_count * 4];
        for alpha in rgba.iter_mut().skip(3).step_by(4) {
            *alpha = 0xFF;
        }
        rgba
    }

    fn run_until_frame_ready(&mut self) {
        while !self.snes.is_ready_to_render() {
            self.snes.run_tick();
        }
        self.snes.clear_ready_to_render();
    }

    fn drain_audio_buffer(&mut self) {
        while self.snes.get_sample().is_some() {}
    }

    #[cfg(all(test, target_arch = "wasm32"))]
    pub(crate) fn joypad_button_states_for_test(&self) -> u8 {
        self.snes.get_joypad_button_states(0)
    }

    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmSnes {
        console_error_panic_hook::set_once();
        let app_context: SharedAppContext = Rc::new(RefCell::new(AppContext::new_with_config(
            Default::default(),
        )));
        WasmSnes {
            snes: Snes::new(app_context),
            audio_muted: false,
            rom_loaded: false,
            pending_toasts: Vec::new(),
        }
    }

    /// Load a SNES ROM from raw bytes.
    #[wasm_bindgen]
    pub fn load_rom(&mut self, rom: &[u8], rom_name: &str) -> Result<(), JsValue> {
        self.rom_loaded = false;
        match self.snes.load_rom(rom, rom_name) {
            Ok(()) => {
                self.rom_loaded = true;
                self.snes.set_audio_sample_rate(44_100.0);
                self.pending_toasts
                    .push(cartridge_load_toast_message(rom_name, true));
                web_sys::console::log_1(&JsValue::from_str("SNES ROM loaded successfully"));
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

    /// Step the emulator until a full frame is ready and return the pixel buffer (RGBA8888).
    ///
    /// Returns a `Uint8Array` of `256 × 224 × 4` bytes.
    /// When no ROM is loaded, returns an opaque black frame.
    #[wasm_bindgen]
    pub fn render_frame_rgba(&mut self) -> Vec<u8> {
        if !self.rom_loaded {
            return Self::opaque_black_rgba_frame();
        }
        self.run_until_frame_ready();
        let screen_width = self.snes.screen_width();
        let screen_height = self.snes.screen_height();
        let rgb = self.snes.screen_snapshot();
        Self::rgba_frame_from_snapshot(
            &rgb,
            screen_width,
            screen_height,
            Snes::SCREEN_WIDTH,
            Snes::SCREEN_HEIGHT,
        )
    }

    /// Returns the display width in pixels (always 256 for SNES).
    #[wasm_bindgen]
    pub fn screen_width(&self) -> u32 {
        Snes::SCREEN_WIDTH
    }

    /// Returns the display height in pixels (always 224 for SNES NTSC).
    #[wasm_bindgen]
    pub fn screen_height(&self) -> u32 {
        Snes::SCREEN_HEIGHT
    }

    /// Returns the nominal SNES NTSC refresh rate in Hz.
    ///
    /// Master clock: 21.477272 MHz, 357,366 cycles per frame ≈ 60.098 Hz.
    #[wasm_bindgen]
    pub fn frame_rate_hz(&self) -> f64 {
        21_477_272.0 / 357_366.0
    }

    /// Collect all pending mono audio samples from the APU.
    ///
    /// Returns a `Float32Array`. Call after each `render_frame_rgba`.
    #[wasm_bindgen]
    pub fn get_audio_samples(&mut self) -> Vec<f32> {
        if self.audio_muted {
            self.drain_audio_buffer();
            return Vec::new();
        }
        let mut samples = Vec::new();
        while let Some(s) = self.snes.get_sample() {
            samples.push(s);
        }
        samples
    }

    /// Collect all pending stereo audio samples from the APU.
    ///
    /// Returns interleaved left/right `Float32Array`. Call after each `render_frame_rgba`.
    #[wasm_bindgen]
    pub fn get_audio_samples_stereo(&mut self) -> Vec<f32> {
        if self.audio_muted {
            self.drain_audio_buffer();
            return Vec::new();
        }
        let mut samples = Vec::new();
        while let Some((left, right)) = self.snes.get_stereo_sample() {
            samples.push(left);
            samples.push(right);
        }
        samples
    }

    /// Set the emulator audio output sample rate in Hz.
    #[wasm_bindgen]
    pub fn set_audio_sample_rate(&mut self, sample_rate: f32) {
        self.snes.set_audio_sample_rate(sample_rate);
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

    /// Set button state for a SNES controller.
    ///
    /// `controller` is the 1-based port number (1 or 2).
    /// `button` uses `crate::snes::input::button_from_id`, i.e. internal IDs `0=A, 1=B, 2=Select, 3=Start, 4=Up, 5=Down, 6=Left, 7=Right, 8=L, 9=R, 10=X, 11=Y`.
    #[wasm_bindgen]
    pub fn set_button(&mut self, controller: u8, button: u8, pressed: bool) {
        if let Some(port) = Self::physical_port(controller) {
            self.snes.set_button(port, button, pressed);
        }
    }

    /// Reset the emulator.
    #[wasm_bindgen]
    pub fn reset(&mut self, soft_reset: bool) {
        self.snes.reset(soft_reset);
    }

    /// Serialize the current emulator state to bytes.
    #[wasm_bindgen]
    pub fn save_state_bytes(&self) -> Vec<u8> {
        self.snes.save_state_bytes().unwrap_or_default()
    }

    /// Restore emulator state from previously serialized bytes.
    #[wasm_bindgen]
    pub fn load_state_bytes(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        self.snes
            .load_state_bytes(bytes)
            .map_err(|e| JsValue::from_str(&e))
    }

    /// Returns `true` if a SNES mouse peripheral is attached on any port.
    #[wasm_bindgen]
    pub fn has_mouse(&self) -> bool {
        self.snes.has_mouse()
    }

    /// Returns `true` if a SNES mouse peripheral is attached on the given 1-based port.
    #[wasm_bindgen]
    pub fn has_mouse_on_port(&self, port: u8) -> bool {
        Self::physical_port(port).is_some_and(|port| self.snes.has_mouse_on_port(port))
    }

    /// Report mouse movement delta to the SNES mouse peripheral on the given 1-based port.
    #[wasm_bindgen]
    pub fn add_mouse_delta(&mut self, port: u8, dx: i16, dy: i16) {
        if let Some(port) = Self::physical_port(port) {
            self.snes.add_mouse_delta(port, dx, dy);
        }
    }

    /// Set the left mouse button state for the given 1-based port.
    #[wasm_bindgen]
    pub fn set_mouse_left_button(&mut self, port: u8, pressed: bool) {
        if let Some(port) = Self::physical_port(port) {
            self.snes.set_mouse_left_button(port, pressed);
        }
    }

    /// Set the right mouse button state for the given 1-based port.
    #[wasm_bindgen]
    pub fn set_mouse_right_button(&mut self, port: u8, pressed: bool) {
        if let Some(port) = Self::physical_port(port) {
            self.snes.set_mouse_right_button(port, pressed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WasmSnes;

    #[test]
    fn js_ports_map_to_physical_snes_ports() {
        assert_eq!(WasmSnes::physical_port(1), Some(0));
        assert_eq!(WasmSnes::physical_port(2), Some(1));
        assert_eq!(WasmSnes::physical_port(0), None);
        assert_eq!(WasmSnes::physical_port(3), None);
    }

    #[test]
    fn rgba_frame_from_snapshot_crops_and_pads_to_target_size() {
        let rgb = vec![
            1, 2, 3, 4, 5, 6, 7, 8, 9, //
            10, 11, 12, 13, 14, 15, 16, 17, 18,
        ];

        let rgba = WasmSnes::rgba_frame_from_snapshot(&rgb, 3, 2, 2, 3);

        assert_eq!(
            rgba,
            vec![
                1, 2, 3, 0xFF, 4, 5, 6, 0xFF, //
                10, 11, 12, 0xFF, 13, 14, 15, 0xFF, //
                0, 0, 0, 0xFF, 0, 0, 0, 0xFF,
            ]
        );
    }
}
