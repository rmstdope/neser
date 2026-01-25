use crate::cartridge::Cartridge;
use crate::console::{Nes, TvSystem};
use crate::input::Button;
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
}
