use crate::cartridge::Cartridge;
use crate::console::{Nes, TvSystem};
use wasm_bindgen::prelude::*;

/// Provides a minimal WASM bridge for running the emulator in the browser.
///
/// Note: NTSC timing is hardcoded for the MVP; PAL titles will run at the wrong speed.
#[wasm_bindgen]
pub struct WasmNes {
    nes: Nes,
}

#[wasm_bindgen]
impl WasmNes {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmNes {
        console_error_panic_hook::set_once();
        WasmNes {
            nes: Nes::new(TvSystem::Ntsc),
        }
    }

    /// Load a ROM from raw bytes.
    #[wasm_bindgen]
    pub fn load_rom(&mut self, rom: &[u8]) -> Result<(), JsValue> {
        let cart = Cartridge::new(rom).map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.nes.insert_cartridge(cart);
        self.nes.reset(false);
        Ok(())
    }

    /// Step the emulator until a full frame is ready and return the pixel buffer (RGB888).
    ///
    /// Returns a Uint8Array with length 256*240*3.
    #[wasm_bindgen]
    pub fn render_frame(&mut self) -> Vec<u8> {
        while !self.nes.is_ready_to_render() {
            self.nes.run_cpu_tick();
        }
        self.nes.clear_ready_to_render();
        self.nes.get_screen_buffer().snapshot()
    }
}
