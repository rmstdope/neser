use crate::gba::Gba;
use crate::platform::app_context::{AppContext, SharedAppContext};
use crate::platform::emulator::Emulator;
use crate::platform::frontend_toasts::cartridge_load_toast_message;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

/// Provides a minimal WASM bridge for running the Game Boy Advance emulator in the browser.
#[wasm_bindgen]
pub struct WasmGba {
    gba: Gba,
    audio_muted: bool,
    rom_loaded: bool,
    pending_toasts: Vec<String>,
    frame_rgba_buffer: Vec<u8>,
    frame_rgb_buffer: Vec<u8>,
}

impl Default for WasmGba {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl WasmGba {
    fn required_rgba_len() -> usize {
        (Gba::SCREEN_WIDTH * Gba::SCREEN_HEIGHT * 4) as usize
    }

    fn ensure_rgba_buffer(&mut self) {
        let required = Self::required_rgba_len();
        if self.frame_rgba_buffer.len() != required {
            self.frame_rgba_buffer.resize(required, 0xFF);
            for alpha in self.frame_rgba_buffer.iter_mut().skip(3).step_by(4) {
                *alpha = 0xFF;
            }
        }
    }

    fn required_rgb_len() -> usize {
        (Gba::SCREEN_WIDTH * Gba::SCREEN_HEIGHT * 3) as usize
    }

    fn fill_black_rgb_frame(&mut self) {
        let required = Self::required_rgb_len();
        if self.frame_rgb_buffer.len() != required {
            self.frame_rgb_buffer.resize(required, 0);
        } else {
            self.frame_rgb_buffer.fill(0);
        }
    }

    fn fill_opaque_black_frame(&mut self) {
        self.ensure_rgba_buffer();
        self.frame_rgba_buffer.fill(0);
        for alpha in self.frame_rgba_buffer.iter_mut().skip(3).step_by(4) {
            *alpha = 0xFF;
        }
    }

    fn run_until_frame_ready(&mut self) {
        while !self.gba.is_ready_to_render() {
            self.gba.run_tick();
        }
        self.gba.clear_ready_to_render();
    }

    fn drain_audio_buffer(&mut self) {
        while self.gba.get_sample().is_some() {}
    }

    #[cfg(all(test, target_arch = "wasm32"))]
    pub(crate) fn joypad_button_states_for_test(&self) -> u8 {
        self.gba.get_joypad_button_states(1)
    }

    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmGba {
        console_error_panic_hook::set_once();
        let app_context: SharedAppContext = Rc::new(RefCell::new(AppContext::new_with_config(
            Default::default(),
        )));
        WasmGba {
            gba: Gba::new(app_context),
            audio_muted: false,
            rom_loaded: false,
            pending_toasts: Vec::new(),
            frame_rgba_buffer: Vec::new(),
            frame_rgb_buffer: Vec::new(),
        }
    }

    #[wasm_bindgen]
    pub fn load_rom(&mut self, rom: &[u8], rom_name: &str) -> Result<(), JsValue> {
        self.rom_loaded = false;
        match self.gba.load_rom(rom, rom_name) {
            Ok(()) => {
                self.rom_loaded = true;
                self.gba.set_audio_sample_rate(44_100.0);
                self.pending_toasts
                    .push(cartridge_load_toast_message(rom_name, true));
                web_sys::console::log_1(&JsValue::from_str("GBA ROM loaded successfully"));
                Ok(())
            }
            Err(err) => {
                self.pending_toasts
                    .push(cartridge_load_toast_message(rom_name, false));
                Err(JsValue::from_str(&err))
            }
        }
    }

    #[wasm_bindgen]
    pub fn drain_toasts(&mut self) -> Vec<JsValue> {
        self.pending_toasts.drain(..).map(JsValue::from).collect()
    }

    /// Step the emulator until a full frame is ready and return the pixel buffer (RGBA8888).
    ///
    /// Returns a `Uint8Array` of `240 × 160 × 4` bytes.
    /// When no ROM is loaded, returns an opaque black frame.
    ///
    /// # Safety
    ///
    /// The returned `Uint8Array` is a zero-copy view into `self.frame_rgba_buffer` in WASM
    /// linear memory. The caller must consume it before invoking another WASM function that could
    /// grow linear memory.
    #[wasm_bindgen]
    pub fn render_frame_rgba(&mut self) -> js_sys::Uint8Array {
        if !self.rom_loaded {
            self.fill_opaque_black_frame();
            return unsafe { js_sys::Uint8Array::view(&self.frame_rgba_buffer) };
        }

        self.run_until_frame_ready();
        self.ensure_rgba_buffer();
        let rgb = self.gba.framebuffer_rgb();
        for (rgba, rgb) in self
            .frame_rgba_buffer
            .chunks_exact_mut(4)
            .zip(rgb.chunks_exact(3))
        {
            rgba[0] = rgb[0];
            rgba[1] = rgb[1];
            rgba[2] = rgb[2];
            rgba[3] = 0xFF;
        }
        unsafe { js_sys::Uint8Array::view(&self.frame_rgba_buffer) }
    }

    /// Step the emulator until a full frame is ready and return the native RGB888 pixel buffer.
    ///
    /// Returns a `Uint8Array` of `240 × 160 × 3` bytes.
    /// When no ROM is loaded, returns a black frame.
    ///
    /// # Safety
    ///
    /// The returned `Uint8Array` is a zero-copy view into WASM linear memory. The caller must
    /// consume it before invoking another WASM function that could grow linear memory.
    #[wasm_bindgen]
    pub fn render_frame_rgb(&mut self) -> js_sys::Uint8Array {
        if !self.rom_loaded {
            self.fill_black_rgb_frame();
            return unsafe { js_sys::Uint8Array::view(&self.frame_rgb_buffer) };
        }

        self.run_until_frame_ready();
        unsafe { js_sys::Uint8Array::view(self.gba.framebuffer_rgb()) }
    }

    #[wasm_bindgen]
    pub fn screen_width(&self) -> u32 {
        Gba::SCREEN_WIDTH
    }

    #[wasm_bindgen]
    pub fn screen_height(&self) -> u32 {
        Gba::SCREEN_HEIGHT
    }

    #[wasm_bindgen]
    pub fn frame_rate_hz(&self) -> f64 {
        16_777_216.0 / 280_896.0
    }

    #[wasm_bindgen]
    pub fn get_audio_samples(&mut self) -> Vec<f32> {
        if self.audio_muted {
            self.drain_audio_buffer();
            return Vec::new();
        }
        let mut samples = Vec::new();
        while let Some(sample) = self.gba.get_sample() {
            samples.push(sample);
        }
        samples
    }

    /// Set the emulator audio output sample rate in Hz.
    #[wasm_bindgen]
    pub fn set_audio_sample_rate(&mut self, sample_rate: f32) {
        self.gba.set_audio_sample_rate(sample_rate);
    }

    #[wasm_bindgen]
    pub fn set_audio_muted(&mut self, muted: bool) {
        self.audio_muted = muted;
        if muted {
            self.drain_audio_buffer();
        }
    }

    #[wasm_bindgen]
    pub fn is_audio_muted(&self) -> bool {
        self.audio_muted
    }

    #[wasm_bindgen]
    pub fn set_button(&mut self, controller: u8, button: u8, pressed: bool) {
        if controller == 1 {
            self.gba.set_button(controller, button, pressed);
        }
    }

    #[wasm_bindgen]
    pub fn reset(&mut self, soft_reset: bool) {
        self.gba.reset(soft_reset);
    }
}
