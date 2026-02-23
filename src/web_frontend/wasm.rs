use crate::app_context::{AppContext, SharedAppContext};
use crate::cartridge::Cartridge;
use crate::console::{Config, Nes, SaveState, log_rom_timing_mode_selection};
use crate::debugging::snapshot as debugger_snapshot;
use crate::frontend_toasts::{
    cartridge_load_toast_message, emulator_timing_toast_message,
    gamepad_init_toast_message as shared_gamepad_init_toast_message,
};
use crate::input::{Button, ControllerType};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

/// Provides a minimal WASM bridge for running the emulator in the browser.
#[wasm_bindgen]
pub struct WasmNes {
    nes: Nes,
    audio_muted: bool,
    rom_loaded: bool,
    pending_toasts: Vec<String>,
    app_context: SharedAppContext,
    /// True while the debugger is open and the emulator is paused.
    debugger_paused: bool,
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

    fn run_until_frame_ready(&mut self) {
        if self.debugger_paused {
            return;
        }
        while !self.nes.is_ready_to_render() {
            self.nes.run_cpu_tick();
        }
        self.nes.clear_ready_to_render();
    }

    fn opaque_black_rgba_frame(pixel_count: usize) -> Vec<u8> {
        let mut rgba = vec![0u8; pixel_count * 4];
        for alpha in rgba.iter_mut().skip(3).step_by(4) {
            *alpha = 0xFF;
        }
        rgba
    }

    fn overscan(&self) -> (u32, u32) {
        let cfg = self.app_context.borrow();
        let config = cfg.config();
        (
            config.horizontal_overscan as u32,
            config.vertical_overscan as u32,
        )
    }

    fn rgb_to_rgba(rgb: &[u8]) -> Vec<u8> {
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

    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmNes {
        console_error_panic_hook::set_once();
        let app_context = Rc::new(RefCell::new(AppContext::new_with_config(Config::default())));
        WasmNes {
            nes: Nes::new(app_context.clone()),
            audio_muted: false,
            rom_loaded: false,
            pending_toasts: Vec::new(),
            app_context,
            debugger_paused: false,
        }
    }

    /// Load a ROM from raw bytes.
    #[wasm_bindgen]
    pub fn load_rom(&mut self, rom: &[u8], rom_name: &str) -> Result<(), JsValue> {
        let app_context = self.app_context.clone();
        {
            *app_context.borrow_mut().config_mut() = Config::default();
        }
        self.rom_loaded = false;
        let cart = match Cartridge::load_from_file(rom, rom_name, app_context.clone()) {
            Ok(cart) => cart,
            Err(err) => {
                self.pending_toasts
                    .push(cartridge_load_toast_message(rom_name, false));
                return Err(JsValue::from_str(&err.to_string()));
            }
        };

        let rom_timing_mode = cart.rom_timing_mode();
        let applied = app_context
            .borrow_mut()
            .config_mut()
            .apply_rom_timing_mode(rom_timing_mode);
        log_rom_timing_mode_selection(&app_context, rom_timing_mode, applied);

        self.nes = Nes::new(app_context);
        self.nes.insert_cartridge(cart);
        self.nes.reset(false);
        self.rom_loaded = true;
        self.pending_toasts
            .push(cartridge_load_toast_message(rom_name, true));
        self.pending_toasts.push(emulator_timing_toast_message(
            self.nes.app_context.borrow().config().tv_system,
        ));
        web_sys::console::log_1(&JsValue::from_str("ROM loaded successfully"));
        Ok(())
    }

    #[wasm_bindgen]
    pub fn drain_toasts(&mut self) -> Vec<JsValue> {
        self.pending_toasts.drain(..).map(JsValue::from).collect()
    }

    /// Reset the emulator without ejecting the cartridge.
    #[wasm_bindgen]
    pub fn reset(&mut self) {
        self.nes.reset(true);
    }

    /// Step the emulator until a full frame is ready and return the pixel buffer (RGB888).
    ///
    /// Returns a Uint8Array with the cropped frame after overscan removal.
    /// Width = 256 - 2*horizontal_overscan, Height = 240 - 2*vertical_overscan.
    ///
    /// When the debugger is open (`is_debugger_open()` returns true), the emulator
    /// is paused and the last rendered frame is returned without advancing.
    #[wasm_bindgen]
    pub fn render_frame(&mut self) -> Vec<u8> {
        if !self.rom_loaded {
            let pixel_count = self.screen_width() as usize * self.screen_height() as usize;
            return vec![0u8; pixel_count * 3];
        }
        let (h, v) = self.overscan();
        self.run_until_frame_ready();
        self.nes.get_screen_buffer().cropped_snapshot(h, v)
    }

    /// Step the emulator until a full frame is ready and return the pixel buffer (RGBA8888).
    ///
    /// Returns a Uint8Array with the cropped frame after overscan removal.
    /// Width = 256 - 2*horizontal_overscan, Height = 240 - 2*vertical_overscan.
    ///
    /// When the debugger is open (`is_debugger_open()` returns true), the emulator
    /// is paused and the last rendered frame is returned without advancing.
    #[wasm_bindgen]
    pub fn render_frame_rgba(&mut self) -> Vec<u8> {
        let pixel_count = self.screen_width() as usize * self.screen_height() as usize;
        if !self.rom_loaded {
            return Self::opaque_black_rgba_frame(pixel_count);
        }
        let (h, v) = self.overscan();
        self.run_until_frame_ready();
        let rgb = self.nes.get_screen_buffer().cropped_snapshot(h, v);
        Self::rgb_to_rgba(&rgb)
    }

    /// Returns the display width in pixels after overscan removal.
    #[wasm_bindgen]
    pub fn screen_width(&self) -> u32 {
        let (h, _) = self.overscan();
        256 - 2 * h
    }

    /// Returns the display height in pixels after overscan removal.
    #[wasm_bindgen]
    pub fn screen_height(&self) -> u32 {
        let (_, v) = self.overscan();
        240 - 2 * v
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

    /// Check if a Zapper light gun is active on the specified port.
    /// Returns true if a Zapper is connected to the port.
    /// This is used by the JavaScript frontend to show/hide the crosshair cursor.
    #[wasm_bindgen]
    pub fn is_zapper_active(&self, port: u8) -> bool {
        self.nes.is_zapper_active(port)
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
    /// The NES has 240 visible scanlines, so the meaningful Y range on screen is 0..=239.
    /// Values in this range will be within the visible area; values >= 240 are forwarded
    /// to the backend but are outside the visible region and will not cause the Zapper
    /// to detect light.
    ///
    /// # Arguments
    /// * `position` - The mouse-emulated controller position value (useful range 0..=239)
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

    /// Get the nominal TV-system refresh rate in Hz for the loaded ROM or system default (if not ROM loaded).
    #[wasm_bindgen]
    pub fn frame_rate_hz(&self) -> f64 {
        self.nes
            .app_context
            .borrow()
            .config()
            .tv_system
            .frame_rate_hz()
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

    // --- Debugger API ---

    /// Returns true if the debugger is currently open (emulator paused).
    #[wasm_bindgen]
    pub fn is_debugger_open(&self) -> bool {
        self.debugger_paused
    }

    /// Open the debugger: pause the emulator.
    #[wasm_bindgen]
    pub fn debugger_open(&mut self) {
        self.debugger_paused = true;
    }

    /// Continue execution: close the debugger and resume the emulator.
    #[wasm_bindgen]
    pub fn debugger_continue(&mut self) {
        self.debugger_paused = false;
    }

    /// Step into: execute one CPU instruction and keep the debugger open.
    #[wasm_bindgen]
    pub fn debugger_step_into(&mut self) {
        self.debugger_paused = true;
        self.nes.run_cpu_tick();
    }

    /// Step over: like step into, but treats JSR as a single unit (runs until the return address).
    #[wasm_bindgen]
    pub fn debugger_step_over(&mut self) {
        self.debugger_paused = true;
        step_over_instruction(&mut self.nes);
    }

    /// Returns the current CPU program counter value (useful for testing step behaviour).
    #[wasm_bindgen]
    pub fn debugger_cpu_pc(&self) -> u16 {
        self.nes.cpu.pc()
    }

    /// Take a snapshot of the current CPU/PPU/APU state and return it as a JSON string.
    ///
    /// The returned JSON contains `pc`, `a`, `x`, `y`, `sp`, `p`, `cycles`, and other fields.
    #[wasm_bindgen]
    pub fn debugger_snapshot_json(&self) -> String {
        let snap = debugger_snapshot(&self.nes);
        // Serialize enough state to be useful; keep it simple without pulling in serde.
        format!(
            r#"{{"pc":{pc},"a":{a},"x":{x},"y":{y},"sp":{sp},"p":{p},"cycles":{cycles},"scanline":{scanline},"pixel":{pixel}}}"#,
            pc = snap.cpu_regs.pc,
            a = snap.cpu_regs.a,
            x = snap.cpu_regs.x,
            y = snap.cpu_regs.y,
            sp = snap.cpu_regs.sp,
            p = snap.cpu_regs.p,
            cycles = snap.cpu_regs.cycles,
            scanline = snap.cpu_regs.scanline,
            pixel = snap.cpu_regs.pixel,
        )
    }

    /// Returns a JSON array of disassembly lines around the current PC.
    ///
    /// Each element is `{"addr":<u16>,"bytes":[<u8>...],"text":"<str>","is_current":<bool>}`.
    #[wasm_bindgen]
    pub fn debugger_disasm_json(&self) -> String {
        let snap = debugger_snapshot(&self.nes);
        let mut json = String::from('[');
        for (i, line) in snap.cpu_disasm.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            json.push_str(&disasm_line_to_json_object(
                line.addr,
                &line.bytes,
                &line.text,
                line.is_current,
            ));
        }
        json.push(']');
        json
    }

    // --- End Debugger API ---

    #[cfg(test)]
    #[wasm_bindgen]
    pub fn push_audio_sample_for_test(&mut self, sample: f32) {
        self.nes.apu.borrow_mut().push_sample_for_test(sample);
    }
}

/// Formats a byte slice as a JSON array string, e.g. `[1,2,3]`.
fn bytes_to_json_array(bytes: &[u8]) -> String {
    let mut b = String::from('[');
    for (j, byte) in bytes.iter().enumerate() {
        if j > 0 {
            b.push(',');
        }
        b.push_str(&byte.to_string());
    }
    b.push(']');
    b
}

/// Formats one disassembly line as a JSON object string.
fn disasm_line_to_json_object(addr: u16, bytes: &[u8], text: &str, is_current: bool) -> String {
    let bytes_json = bytes_to_json_array(bytes);
    let escaped_text = text.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        r#"{{"addr":{},"bytes":{},"text":"{}","is_current":{}}}"#,
        addr, bytes_json, escaped_text, is_current
    )
}

#[wasm_bindgen]
pub fn gamepad_init_toast_message(gamepads_enabled: bool, detected_controllers: usize) -> String {
    shared_gamepad_init_toast_message(gamepads_enabled, detected_controllers)
}

/// Execute a single step-over operation on the NES CPU.
///
/// If the current instruction is a JSR ($20), runs until the instruction at
/// the return address (PC + 3) is reached; otherwise executes one CPU tick.
fn step_over_instruction(nes: &mut Nes) {
    const JSR_OPCODE: u8 = 0x20;
    const MAX_STEPS: usize = 1_000_000;

    let pc = nes.cpu.pc();
    let opcode = nes.bus.borrow().read_cpu_for_debugger(pc);

    if opcode == JSR_OPCODE {
        let next_pc = pc.wrapping_add(3);
        nes.run_cpu_tick(); // enter the subroutine
        for _ in 0..MAX_STEPS {
            if nes.cpu.pc() == next_pc || nes.cpu.is_halted() {
                break;
            }
            nes.run_cpu_tick();
        }
    } else {
        nes.run_cpu_tick();
    }
}
