use crate::gba::Gba;
use crate::platform::app_context::{AppContext, SharedAppContext};
use crate::platform::emulator::Emulator;
use crate::web_console::{WebConsole, web_console_bindings};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

/// Provides a minimal WASM bridge for running the Game Boy Advance emulator in the browser.
#[wasm_bindgen]
pub struct WasmGba {
    web: WebConsole<Gba>,
    frame_rgba_buffer: Vec<u8>,
    frame_rgb_buffer: Vec<u8>,
}

web_console_bindings!(WasmGba);

impl Default for WasmGba {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl WasmGba {
    fn create_with_skip_bios_intro(skip_bios_intro: bool) -> WasmGba {
        console_error_panic_hook::set_once();
        let mut config = crate::platform::config::Config::default();
        config.gba.skip_bios_intro = skip_bios_intro;
        let app_context: SharedAppContext =
            Rc::new(RefCell::new(AppContext::new_with_config(config)));
        WasmGba {
            web: WebConsole::new(Gba::new(app_context)),
            frame_rgba_buffer: Vec::new(),
            frame_rgb_buffer: Vec::new(),
        }
    }

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

    #[cfg(all(test, target_arch = "wasm32"))]
    pub(crate) fn joypad_button_states_for_test(&self) -> u8 {
        self.web.core().get_joypad_button_states(1)
    }

    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmGba {
        Self::create_with_skip_bios_intro(false)
    }

    #[wasm_bindgen]
    pub fn new_with_skip_bios_intro(skip_bios_intro: bool) -> WasmGba {
        Self::create_with_skip_bios_intro(skip_bios_intro)
    }

    #[wasm_bindgen]
    pub fn load_rom(&mut self, rom: &[u8], rom_name: &str) -> Result<(), JsValue> {
        self.web
            .load_rom(rom, rom_name)
            .map_err(|err| JsValue::from_str(&err))?;
        web_sys::console::log_1(&JsValue::from_str("GBA ROM loaded successfully"));
        Ok(())
    }

    /// The Colors button's label: the colour correction's state in the words
    /// of its corner message.
    #[wasm_bindgen]
    pub fn color_label(&self) -> String {
        self.web.core().color_correction_label()
    }

    /// Whether the GBA LCD colour correction is on.
    #[wasm_bindgen]
    pub fn color_correction(&self) -> bool {
        self.web.core().color_correction()
    }

    /// Turn the GBA LCD colour correction on or off, e.g. to carry the page's
    /// choice into a new instance. Kept for any game later loaded into it.
    #[wasm_bindgen]
    pub fn set_color_correction(&mut self, enabled: bool) {
        self.web.core_mut().set_color_correction(enabled);
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
        if !self.web.rom_loaded() {
            self.fill_opaque_black_frame();
            return unsafe { js_sys::Uint8Array::view(&self.frame_rgba_buffer) };
        }

        self.web.run_until_frame_ready();
        self.ensure_rgba_buffer();
        let rgb = self.web.core().framebuffer_rgb();
        for (rgba, rgb) in self
            .frame_rgba_buffer
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(rgb.as_chunks::<3>().0)
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
        if !self.web.rom_loaded() {
            self.fill_black_rgb_frame();
            return unsafe { js_sys::Uint8Array::view(&self.frame_rgb_buffer) };
        }

        self.web.run_until_frame_ready();
        unsafe { js_sys::Uint8Array::view(self.web.core().framebuffer_rgb()) }
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

    /// Collect all pending stereo audio samples, interleaved left/right; empty while muted.
    #[wasm_bindgen]
    pub fn get_audio_samples_stereo(&mut self) -> Vec<f32> {
        self.web.audio_samples_stereo()
    }

    #[wasm_bindgen]
    pub fn set_button(&mut self, controller: u8, button: u8, pressed: bool) {
        if controller == 1 {
            self.web.core_mut().set_button(controller, button, pressed);
        }
    }

    #[wasm_bindgen]
    pub fn reset(&mut self, soft_reset: bool) {
        self.web.reset(soft_reset);
    }
}
