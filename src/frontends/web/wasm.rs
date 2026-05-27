use crate::nes::cartridge::Cartridge;
use crate::nes::console::{Config, Nes, SaveState, log_hardware_selection};
use crate::nes::debugging::DebuggerViewState;
use crate::nes::debugging::ppu_viewer::{
    PpuViewerSnapshot, render_nametables_rgba, render_pattern_tables_rgba,
};
use crate::nes::frontend_toasts::{emulator_timing_toast_message, hardware_mode_toast_message};
use crate::nes::input::{Button, ControllerType, SnesButton};
use crate::platform::app_context::{AppContext, SharedAppContext};
use crate::platform::autorun::crc32;
use crate::platform::frontend_toasts::{
    cartridge_load_toast_message, gamepad_init_toast_message as shared_gamepad_init_toast_message,
};
use crate::wasm_autorun::WasmAutorunState;
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
    /// Stateful debugger view state for disassembly window persistence.
    debugger_view_state: DebuggerViewState,
    /// Current autorun recording or playback state.
    autorun_state: Option<WasmAutorunState>,
    /// Bitmask of currently pressed buttons on controller 1 (for autorun recording).
    controller1_buttons: u8,
    /// Bitmask of currently pressed buttons on controller 2 (for autorun recording).
    controller2_buttons: u8,
    /// Pre-allocated RGBA frame buffer; reused every frame to avoid per-frame heap allocation.
    frame_rgba_buffer: Vec<u8>,
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

    fn overscan(&self) -> (u32, u32) {
        let cfg = self.app_context.borrow();
        let config = cfg.config();
        (
            config.nes.horizontal_overscan as u32,
            config.nes.vertical_overscan as u32,
        )
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
            debugger_view_state: DebuggerViewState::default(),
            autorun_state: None,
            controller1_buttons: 0,
            controller2_buttons: 0,
            frame_rgba_buffer: Vec::new(),
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
        let cart = match Cartridge::load_from_file(rom, rom_name, Some(self.nes.rom_db())) {
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
        self.nes = Nes::new(app_context.clone());
        self.nes.insert_cartridge(cart);
        log_hardware_selection(&app_context, applied);
        self.nes.reset(false);
        self.debugger_view_state = DebuggerViewState::default();
        self.rom_loaded = true;
        self.pending_toasts
            .push(cartridge_load_toast_message(rom_name, true));
        self.pending_toasts.push(emulator_timing_toast_message(
            self.nes
                .app_context()
                .borrow()
                .config()
                .nes
                .hardware_model
                .timing_mode(),
        ));
        {
            let config = self.nes.app_context().borrow().config().clone();
            self.pending_toasts.push(hardware_mode_toast_message(
                config.nes.hardware_mode,
                config.nes.hardware_model,
                config.nes.expansion_port,
                config.nes.four_score_enabled,
            ));
        }
        web_sys::console::log_1(&JsValue::from_str("ROM loaded successfully"));
        Ok(())
    }

    #[wasm_bindgen]
    pub fn drain_toasts(&mut self) -> Vec<JsValue> {
        self.pending_toasts.drain(..).map(JsValue::from).collect()
    }

    /// Reset the emulator without ejecting the cartridge.
    ///
    /// `soft_reset = true` performs a soft reset.
    /// `soft_reset = false` performs a hard reset.
    #[wasm_bindgen]
    pub fn reset(&mut self, soft_reset: bool) {
        self.nes.reset(soft_reset);
        self.debugger_view_state = DebuggerViewState::default();
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
    ///
    /// # Safety
    ///
    /// The returned `Uint8Array` is a zero-copy view into `self.frame_rgba_buffer` in WASM
    /// linear memory.  The caller **must not** invoke any WASM function (which could trigger a
    /// heap growth) between receiving this value and reading from it.  In the browser render
    /// loop the view is consumed synchronously by `texSubImage2D` before the next WASM call,
    /// so this invariant is satisfied.
    #[wasm_bindgen]
    pub fn render_frame_rgba(&mut self) -> js_sys::Uint8Array {
        let (h, v) = self.overscan();
        let dst_w = (256u32 - 2 * h) as usize;
        let dst_h = (240u32 - 2 * v) as usize;
        let required = dst_w * dst_h * 4;

        if self.frame_rgba_buffer.len() != required {
            // Resize and pre-fill alpha; happens only on first call or overscan config change.
            self.frame_rgba_buffer.resize(required, 0xFF);
            for chunk in self.frame_rgba_buffer.chunks_exact_mut(4) {
                chunk[3] = 0xFF;
            }
        }

        if !self.rom_loaded {
            self.frame_rgba_buffer.fill(0);
            for chunk in self.frame_rgba_buffer.chunks_exact_mut(4) {
                chunk[3] = 0xFF;
            }
            // SAFETY: see doc comment above.
            return unsafe { js_sys::Uint8Array::view(&self.frame_rgba_buffer) };
        }

        // ── Autorun pre-frame: extract pre-recorded input (borrow dropped before injection) ──
        if let Some(ref mut state) = self.autorun_state {
            state.begin_frame();
        }
        let prerecorded = if let Some(ref mut state) = self.autorun_state {
            if state.is_extending_playback() || state.is_playback() {
                state.next_playback_frame()
            } else {
                None
            }
        } else {
            None
        };
        if let Some(frame) = prerecorded {
            // Borrow is released now; we can call self methods freely.
            self.inject_autorun_buttons(frame.player1, frame.player2);
        }

        self.run_until_frame_ready();

        if self.autorun_state.is_some() {
            // Autorun path: compute CRC from RGB for compatibility with existing recordings.
            let rgb = self.nes.get_screen_buffer().cropped_snapshot(h, v);
            let screen_crc = crc32(&rgb);

            // ── Autorun post-frame: record or verify ─────────────────────────────────────────
            // Phase 1: record the frame or check the CRC; capture whether a checkpoint is needed.
            let needs_checkpoint = if let Some(ref mut state) = self.autorun_state {
                if state.is_recording() && !state.is_extending_playback() {
                    let p1 = self.controller1_buttons;
                    let p2 = self.controller2_buttons;
                    state.record_frame(p1, p2)
                } else {
                    state.check_playback_checkpoint(screen_crc);
                    false
                }
            } else {
                false
            };

            // Phase 2: if a checkpoint is needed, gather NES state and store it.
            if needs_checkpoint {
                let state_bytes = self.nes.save_state().to_bytes().unwrap_or_default();
                if let Some(ref mut state) = self.autorun_state {
                    state.record_checkpoint(screen_crc, state_bytes);
                }
            }

            // Fill pre-alloc buffer from the already-computed RGB snapshot.
            Self::fill_rgba_buffer_from_rgb(&rgb, &mut self.frame_rgba_buffer);
        } else {
            // Fast path: write crop + RGB→RGBA directly into the pre-alloc buffer.
            // No intermediate Vec is allocated.
            self.nes
                .get_screen_buffer()
                .write_cropped_rgba_into(h, v, &mut self.frame_rgba_buffer);
        }

        // SAFETY: see doc comment on this function.
        unsafe { js_sys::Uint8Array::view(&self.frame_rgba_buffer) }
    }

    /// Fill `out` with RGBA8888 data derived from a packed RGB888 slice.
    /// `out` is resized to `rgb.len() / 3 * 4` bytes.  Alpha is always `0xFF`.
    fn fill_rgba_buffer_from_rgb(rgb: &[u8], out: &mut Vec<u8>) {
        let pixel_count = rgb.len() / 3;
        let required = pixel_count * 4;
        if out.len() != required {
            out.resize(required, 0xFF);
        }
        for i in 0..pixel_count {
            let src = i * 3;
            let dst = i * 4;
            out[dst] = rgb[src];
            out[dst + 1] = rgb[src + 1];
            out[dst + 2] = rgb[src + 2];
            out[dst + 3] = 0xFF;
        }
    }

    /// Inject controller buttons directly into the NES without affecting tracking bitmasks.
    fn inject_autorun_buttons(&mut self, controller1: u8, controller2: u8) {
        for bit in 0..8u8 {
            let btn = match bit {
                0 => Button::A,
                1 => Button::B,
                2 => Button::Select,
                3 => Button::Start,
                4 => Button::Up,
                5 => Button::Down,
                6 => Button::Left,
                _ => Button::Right,
            };
            self.nes.set_button(1, btn, controller1 & (1 << bit) != 0);
            self.nes.set_button(2, btn, controller2 & (1 << bit) != 0);
        }
    }

    /// Start autorun recording mode.
    ///
    /// Call before `load_rom`.  Every subsequent frame rendered by `render_frame_rgba`
    /// will be recorded.  Retrieve the recording with `stop_autorun`.
    #[wasm_bindgen]
    pub fn start_autorun_recording(&mut self) {
        self.autorun_state = Some(WasmAutorunState::new_recording());
    }

    /// Load an autorun file for playback (or extend-recording).
    ///
    /// # Arguments
    /// * `bytes`          – JSON-serialized AutorunFile bytes.
    /// * `checkpoint_idx` – 0-based checkpoint to seek to, or -1 for "from beginning".
    /// * `extend`         – `true` to play back and then continue recording.
    ///
    /// Returns the save-state bytes that the caller must restore before running the
    /// first frame.  Returns an empty slice when no restore is needed.
    #[wasm_bindgen]
    pub fn load_autorun_playback(
        &mut self,
        bytes: &[u8],
        checkpoint_idx: i32,
        extend: bool,
    ) -> Result<Vec<u8>, JsValue> {
        let cp_idx = if checkpoint_idx < 0 {
            None
        } else {
            Some(checkpoint_idx as u32)
        };
        let (state, pending) = WasmAutorunState::new_playback(bytes, cp_idx, extend)
            .map_err(|e| JsValue::from_str(&e))?;
        self.autorun_state = Some(state);
        Ok(pending.unwrap_or_default())
    }

    /// Finalize autorun recording and return the serialized file bytes.
    ///
    /// Appends a final checkpoint with the current screen CRC and emulator state,
    /// then clears the autorun state.  The returned bytes can be offered to the user
    /// as a browser download.
    ///
    /// Returns an empty `Vec` if no recording is active.
    #[wasm_bindgen]
    pub fn stop_autorun(&mut self) -> Vec<u8> {
        if self
            .autorun_state
            .as_ref()
            .map(|s| !s.is_recording())
            .unwrap_or(true)
        {
            self.autorun_state = None;
            return Vec::new();
        }
        // Compute screen CRC and save state before borrowing autorun_state mutably.
        let screen_crc = {
            let (h, v) = self.overscan();
            let rgb = self.nes.get_screen_buffer().cropped_snapshot(h, v);
            crc32(&rgb)
        };
        let save_state_bytes = self.nes.save_state().to_bytes().unwrap_or_default();
        let bytes = if let Some(ref mut state) = self.autorun_state {
            state.finalize_recording(screen_crc, save_state_bytes)
        } else {
            Vec::new()
        };
        self.autorun_state = None;
        bytes
    }

    /// Clear any active autorun state without finalizing a recording.
    #[wasm_bindgen]
    pub fn clear_autorun(&mut self) {
        self.autorun_state = None;
    }

    /// Returns `true` if an autorun (recording or playback) is currently active.
    #[wasm_bindgen]
    pub fn is_autorun_active(&self) -> bool {
        self.autorun_state.is_some()
    }

    /// Returns `true` if the emulator is currently recording an autorun.
    #[wasm_bindgen]
    pub fn autorun_is_recording(&self) -> bool {
        self.autorun_state
            .as_ref()
            .map(|s| s.is_recording())
            .unwrap_or(false)
    }

    /// Returns `true` if the emulator is currently playing back an autorun.
    #[wasm_bindgen]
    pub fn autorun_is_playback(&self) -> bool {
        self.autorun_state
            .as_ref()
            .map(|s| s.is_playback())
            .unwrap_or(false)
    }

    /// Returns `true` when pure playback has exhausted all recorded frames.
    ///
    /// The JavaScript layer should call this after each `render_frame_rgba` and
    /// stop emulation when it returns `true`.
    #[wasm_bindgen]
    pub fn autorun_playback_finished(&self) -> bool {
        self.autorun_state
            .as_ref()
            .map(|s| s.is_playback_finished())
            .unwrap_or(false)
    }

    /// Returns the current recording frame index (0 when not recording).
    ///
    /// Used by the web frontend to display a "REC : MM:SS" overlay.
    #[wasm_bindgen]
    pub fn autorun_recording_frame_count(&self) -> u32 {
        self.autorun_state
            .as_ref()
            .filter(|s| s.is_recording())
            .map(|s| s.current_frame_index() as u32)
            .unwrap_or(0)
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
    /// * `controller` - Controller number (1-4 when Four Score is enabled, otherwise 1-2)
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

        // Track button state bitmask for autorun recording
        let bitmask = match controller {
            1 => &mut self.controller1_buttons,
            2 => &mut self.controller2_buttons,
            _ => return,
        };
        if pressed {
            *bitmask |= 1 << button;
        } else {
            *bitmask &= !(1 << button);
        }
    }

    /// Set SNES button state for a controller.
    ///
    /// # Arguments
    /// * `controller` - Controller number (1-2)
    /// * `button` - SNES button number
    ///   (0=B, 1=Y, 2=Select, 3=Start, 4=Up, 5=Down, 6=Left, 7=Right, 8=A, 9=X, 10=L, 11=R)
    /// * `pressed` - true if pressed, false if released
    ///
    /// # Returns
    /// `true` if the active controller on the port supports SNES button input,
    /// `false` otherwise.
    #[wasm_bindgen]
    pub fn set_snes_button(&mut self, controller: u8, button: u8, pressed: bool) -> bool {
        let snes_button = match button {
            0 => SnesButton::B,
            1 => SnesButton::Y,
            2 => SnesButton::Select,
            3 => SnesButton::Start,
            4 => SnesButton::Up,
            5 => SnesButton::Down,
            6 => SnesButton::Left,
            7 => SnesButton::Right,
            8 => SnesButton::A,
            9 => SnesButton::X,
            10 => SnesButton::L,
            11 => SnesButton::R,
            _ => return false,
        };

        self.nes.set_snes_button(controller, snes_button, pressed)
    }

    #[wasm_bindgen]
    pub fn is_four_score_enabled(&self) -> bool {
        self.nes
            .app_context()
            .borrow()
            .config()
            .nes
            .four_score_enabled
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
            .bus()
            .borrow_mut()
            .set_controller_type(port, controller_type);
        Ok(())
    }

    /// Set the hardware mode: "nes-ntsc", "nes-pal", or "famicom".
    ///
    /// This updates the configuration and controller modes only; it does not
    /// retroactively change the timing or PPU behavior of the currently
    /// running emulator instance. The new hardware mode will take effect
    /// on the next ROM load or emulator reset that re-initializes the core.
    #[wasm_bindgen]
    pub fn set_hardware_mode(&mut self, mode: &str) -> Result<(), JsValue> {
        let app_context = self.app_context.clone();
        {
            let mut ctx = app_context.borrow_mut();
            ctx.config_mut()
                .apply_hardware_value(mode)
                .map_err(|e| JsValue::from_str(&e))?;
        }
        self.nes
            .bus()
            .borrow_mut()
            .sync_controller_modes_from_config();

        // Inform the frontend that the hardware mode was updated in the
        // configuration, and that it will apply on the next ROM load.
        {
            let config = app_context.borrow().config().clone();
            self.pending_toasts.push(hardware_mode_toast_message(
                config.nes.hardware_mode,
                config.nes.hardware_model,
                config.nes.expansion_port,
                config.nes.four_score_enabled,
            ));
        }

        Ok(())
    }

    /// Set the expansion port: "none", "famicom-four-players", "arkanoid", "zapper", or "power-pad".
    #[wasm_bindgen]
    pub fn set_expansion_port(&mut self, port: &str) -> Result<(), JsValue> {
        let app_context = self.app_context.clone();
        {
            let mut ctx = app_context.borrow_mut();
            ctx.config_mut()
                .apply_expansion_port_value(port)
                .map_err(|e| JsValue::from_str(&e))?;
        }
        self.nes
            .bus()
            .borrow_mut()
            .sync_controller_modes_from_config();
        Ok(())
    }

    /// Get the current hardware mode as a string.
    #[wasm_bindgen]
    pub fn get_hardware_mode(&self) -> String {
        let config = self.app_context.borrow().config().clone();
        match config.nes.hardware_mode {
            crate::nes::console::HardwareMode::Nes => {
                config.nes.hardware_model.as_str().to_string()
            }
            crate::nes::console::HardwareMode::Famicom => "famicom".to_string(),
        }
    }

    /// Get the current expansion port as a string.
    #[wasm_bindgen]
    pub fn get_expansion_port(&self) -> String {
        match self.app_context.borrow().config().nes.expansion_port {
            crate::nes::console::ExpansionPort::None => "none".to_string(),
            crate::nes::console::ExpansionPort::FamicomFourPlayers => {
                "famicom-four-players".to_string()
            }
            crate::nes::console::ExpansionPort::ArkanoidFamicom => "arkanoid".to_string(),
            crate::nes::console::ExpansionPort::ZapperFamicom => "zapper".to_string(),
            crate::nes::console::ExpansionPort::PowerPadFamicom => "power-pad".to_string(),
            crate::nes::console::ExpansionPort::VsSystem => "vs-system".to_string(),
            crate::nes::console::ExpansionPort::Playchoice10 => "playchoice10".to_string(),
        }
    }

    /// Check if mouse-emulated controller input is enabled on a port.
    /// Returns true if a mouse-emulated controller is active on the specified port.
    /// This is used by the JavaScript frontend to determine whether to suppress joypad input for that port.
    #[wasm_bindgen]
    pub fn is_mouse_emulated_controller(&self, port: u8) -> bool {
        self.nes.controller_input_type(port) == Some(crate::nes::input::ControllerInput::Mouse)
    }

    /// Check if a Super NES mouse is active on a specific port.
    #[wasm_bindgen]
    pub fn is_snes_mouse_active(&self, port: u8) -> bool {
        (1..=2).contains(&port) && self.nes.has_snes_mouse()
    }

    /// Check if the expansion port has a mouse-controlled device (e.g. Famicom Arkanoid).
    #[wasm_bindgen]
    pub fn has_expansion_mouse_controller(&self) -> bool {
        self.nes.has_expansion_mouse_controller()
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

    /// Set the mouse right button state for any mouse-emulated controller.
    #[wasm_bindgen]
    pub fn set_mouse_right_button(&mut self, pressed: bool) {
        self.nes.set_mouse_right_button(pressed);
    }

    /// Get the nominal TV-system refresh rate in Hz for the loaded ROM or system default (if not ROM loaded).
    #[wasm_bindgen]
    pub fn frame_rate_hz(&self) -> f64 {
        self.nes
            .app_context()
            .borrow()
            .config()
            .nes
            .hardware_model
            .timing_mode()
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

    /// Set the emulator audio output sample rate in Hz.
    #[wasm_bindgen]
    pub fn set_audio_sample_rate(&mut self, sample_rate: f32) {
        self.nes.set_audio_sample_rate(sample_rate);
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
        self.nes.set_cpu_trace_enabled(true);
        self.debugger_paused = true;
    }

    /// Continue execution: close the debugger and resume the emulator.
    #[wasm_bindgen]
    pub fn debugger_continue(&mut self) {
        self.nes.set_cpu_trace_enabled(false);
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

    /// Run until the next frame boundary is crossed and keep the debugger open.
    #[wasm_bindgen]
    pub fn debugger_run_to_next_frame(&mut self) {
        self.debugger_paused = true;
        run_to_next_frame(&mut self.nes);
    }

    /// Run until the scanline changes and keep the debugger open.
    #[wasm_bindgen]
    pub fn debugger_run_to_next_scanline(&mut self) {
        self.debugger_paused = true;
        run_to_next_scanline(&mut self.nes);
    }

    /// Run until the next NMI handler entry and keep the debugger open.
    #[wasm_bindgen]
    pub fn debugger_run_to_nmi(&mut self) {
        self.debugger_paused = true;
        run_to_interrupt_entry(&mut self.nes, 0xFFFA, crate::nes::cpu::InterruptKind::Nmi);
    }

    /// Run until the next IRQ handler entry and keep the debugger open.
    #[wasm_bindgen]
    pub fn debugger_run_to_irq(&mut self) {
        self.debugger_paused = true;
        run_to_interrupt_entry(&mut self.nes, 0xFFFE, crate::nes::cpu::InterruptKind::Irq);
    }

    /// Returns the current CPU program counter value (useful for testing step behaviour).
    #[wasm_bindgen]
    pub fn debugger_cpu_pc(&self) -> u16 {
        self.nes.cpu_ref().pc()
    }

    /// Take a snapshot of the current CPU/PPU/APU state and return it as a JSON string.
    ///
    /// Fields: `pc`, `a`, `x`, `y`, `sp`, `p`, `cycles`, `scanline`, `pixel`,
    /// `frame_count`, `interrupt` (null | "nmi" | "irq"),
    /// `nmi_vector`, `reset_vector`, `irq_vector`,
    /// `prg_hexdump_base`, `prg_hexdump_bytes`, `oam` (256-element array).
    #[wasm_bindgen]
    pub fn debugger_snapshot_json(&mut self) -> String {
        let snap = self.debugger_view_state.snapshot(&self.nes);
        serialize_debugger_snapshot_json(&snap)
    }

    /// Move the PRG-ROM hexdump base 16 bytes backward.
    #[wasm_bindgen]
    pub fn debugger_hexdump_prev_16(&mut self) {
        let visible_base = self
            .debugger_view_state
            .snapshot(&self.nes)
            .prg_hexdump_base;
        self.debugger_view_state
            .nudge_prg_hexdump_base_by_bytes_from(visible_base, -16);
    }

    /// Move the PRG-ROM hexdump base 16 bytes forward.
    #[wasm_bindgen]
    pub fn debugger_hexdump_next_16(&mut self) {
        let visible_base = self
            .debugger_view_state
            .snapshot(&self.nes)
            .prg_hexdump_base;
        self.debugger_view_state
            .nudge_prg_hexdump_base_by_bytes_from(visible_base, 16);
    }

    /// Jump the PRG-ROM hexdump to a specific base address.
    #[wasm_bindgen]
    pub fn debugger_hexdump_set_base(&mut self, base: u16) {
        self.debugger_view_state.set_prg_hexdump_base(base);
    }

    /// Add a CPU memory address to the debugger watch list.
    #[wasm_bindgen]
    pub fn debugger_watch_add(&mut self, address: u16) {
        self.debugger_view_state.add_watch_address(address);
    }

    /// Remove a watch entry by index.
    #[wasm_bindgen]
    pub fn debugger_watch_remove(&mut self, index: usize) {
        self.debugger_view_state.remove_watch_address(index);
    }

    /// Update a watch entry address by index.
    #[wasm_bindgen]
    pub fn debugger_watch_update(&mut self, index: usize, address: u16) {
        self.debugger_view_state
            .update_watch_address(index, address);
    }

    /// Returns a JSON array of disassembly lines around the current PC.
    ///
    /// Each element is `{"addr":<u16>,"bytes":[<u8>...],"text":"<str>","is_current":<bool>}`.
    #[wasm_bindgen]
    pub fn debugger_disasm_json(&mut self) -> String {
        let snap = self.debugger_view_state.snapshot(&self.nes);
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

    /// Returns true if the PPU viewer is currently visible inside the debugger.
    #[wasm_bindgen]
    pub fn debugger_is_ppu_viewer_open(&self) -> bool {
        self.debugger_view_state.is_ppu_viewer_visible()
    }

    /// Toggle PPU viewer visibility in the debugger.
    #[wasm_bindgen]
    pub fn debugger_toggle_ppu_viewer(&mut self) {
        self.debugger_view_state.toggle_ppu_viewer();
    }

    fn ppu_viewer_snapshot(&self) -> PpuViewerSnapshot {
        PpuViewerSnapshot::from_nes(&self.nes)
    }

    /// Returns the PPU pattern table viewer image as RGBA bytes.
    #[wasm_bindgen]
    pub fn debugger_ppu_pattern_tables_rgba(&mut self) -> Vec<u8> {
        let snapshot = self.ppu_viewer_snapshot();
        render_pattern_tables_rgba(&snapshot.chr, &snapshot.palette)
    }

    /// Returns the PPU nametable viewer image as RGBA bytes.
    #[wasm_bindgen]
    pub fn debugger_ppu_nametables_rgba(&mut self) -> Vec<u8> {
        let snapshot = self.ppu_viewer_snapshot();
        render_nametables_rgba(
            &snapshot.chr,
            &snapshot.nametables,
            &snapshot.palette,
            snapshot.bg_pattern_table,
        )
    }

    /// Returns PPU scroll in nametable space as JSON: {"scroll_x":<u16>,"scroll_y":<u16>}.
    #[wasm_bindgen]
    pub fn debugger_ppu_scroll_json(&mut self) -> String {
        let snapshot = self.ppu_viewer_snapshot();
        format!(
            r#"{{"scroll_x":{},"scroll_y":{}}}"#,
            snapshot.scroll.0, snapshot.scroll.1
        )
    }

    // --- End Debugger API ---

    #[cfg(test)]
    #[wasm_bindgen]
    pub fn push_audio_sample_for_test(&mut self, sample: f32) {
        self.nes.apu().borrow_mut().push_sample_for_test(sample);
    }
}

/// Returns the JSON representation of an `Option<InterruptKind>` value.
///
/// Produces `null`, `"nmi"`, or `"irq"` — ready to embed verbatim in a JSON string.
fn interrupt_to_json_str(interrupt: Option<crate::nes::cpu::InterruptKind>) -> &'static str {
    use crate::nes::cpu::InterruptKind;
    match interrupt {
        None => "null",
        Some(InterruptKind::Nmi) => "\"nmi\"",
        Some(InterruptKind::Irq) => "\"irq\"",
    }
}

/// Serialises the CPU register state from a [`DebuggerSnapshot`] to a JSON object string
/// without pulling in serde.
///
/// The snapshot is accepted (rather than the inner `cpu_regs` directly) because
/// `CpuRegsSnapshot` is a private type.
fn serialize_debugger_snapshot_json(snap: &crate::nes::debugging::DebuggerSnapshot) -> String {
    let r = snap.cpu_regs;
    let interrupt = interrupt_to_json_str(r.interrupt);
    let prg_hexdump_bytes = bytes_to_json_array(&snap.prg_hexdump_bytes);
    let oam = bytes_to_json_array(&snap.oam);
    let watch_values = watch_values_to_json_array(snap);
    let recent_trace = recent_trace_to_json_array(snap);
    format!(
        r#"{{"pc":{pc},"a":{a},"x":{x},"y":{y},"sp":{sp},"p":{p},"cycles":{cycles},"scanline":{scanline},"pixel":{pixel},"frame_count":{frame_count},"interrupt":{interrupt},"nmi_vector":{nmi_vector},"reset_vector":{reset_vector},"irq_vector":{irq_vector},"prg_hexdump_base":{prg_hexdump_base},"prg_hexdump_bytes":{prg_hexdump_bytes},"oam":{oam},"watch_values":{watch_values},"recent_trace":{recent_trace}}}"#,
        pc = r.pc,
        a = r.a,
        x = r.x,
        y = r.y,
        sp = r.sp,
        p = r.p,
        cycles = r.cycles,
        scanline = r.scanline,
        pixel = r.pixel,
        frame_count = r.frame_count,
        interrupt = interrupt,
        nmi_vector = r.nmi_vector,
        reset_vector = r.reset_vector,
        irq_vector = r.irq_vector,
        prg_hexdump_base = snap.prg_hexdump_base,
        prg_hexdump_bytes = prg_hexdump_bytes,
        oam = oam,
        watch_values = watch_values,
        recent_trace = recent_trace,
    )
}

fn watch_values_to_json_array(snap: &crate::nes::debugging::DebuggerSnapshot) -> String {
    let mut json = String::from("[");
    for (index, entry) in snap.watch_values.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str(&format!(
            r#"{{"address":{},"value":{}}}"#,
            entry.address, entry.value
        ));
    }
    json.push(']');
    json
}

fn recent_trace_to_json_array(snap: &crate::nes::debugging::DebuggerSnapshot) -> String {
    let mut json = String::from("[");
    for (index, entry) in snap.recent_trace.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str(&format!(
            r#"{{"addr":{},"bytes":{},"text":"{}"}}"#,
            entry.addr,
            bytes_to_json_array(&entry.bytes),
            entry.text.replace('\\', "\\\\").replace('"', "\\\"")
        ));
    }
    json.push(']');
    json
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

    let pc = nes.cpu_ref().pc();
    let opcode = nes.bus().borrow().read_cpu_for_debugger(pc);

    if opcode == JSR_OPCODE {
        let next_pc = pc.wrapping_add(3);
        nes.run_cpu_tick(); // enter the subroutine
        for _ in 0..MAX_STEPS {
            if nes.cpu_ref().pc() == next_pc || nes.cpu_ref().is_halted() {
                break;
            }
            nes.run_cpu_tick();
        }
    } else {
        nes.run_cpu_tick();
    }
}

fn run_to_next_frame(nes: &mut Nes) {
    const MAX_STEPS: usize = 2_000_000;

    let mut previous_scanline = {
        let ppu = nes.ppu().borrow();
        ppu.scanline()
    };

    for _step in 0..MAX_STEPS {
        if nes.cpu_ref().is_halted() {
            break;
        }

        nes.run_cpu_tick();

        let scanline = {
            let ppu = nes.ppu().borrow();
            ppu.scanline()
        };

        if scanline < previous_scanline {
            break;
        }

        previous_scanline = scanline;
    }
}

fn run_to_next_scanline(nes: &mut Nes) {
    const MAX_STEPS: usize = 100_000;

    let start_scanline = {
        let ppu = nes.ppu().borrow();
        ppu.scanline()
    };

    for _step in 0..MAX_STEPS {
        if nes.cpu_ref().is_halted() {
            break;
        }

        nes.run_cpu_tick();

        let scanline = {
            let ppu = nes.ppu().borrow();
            ppu.scanline()
        };

        if scanline != start_scanline {
            break;
        }
    }
}

fn read_vector_target(nes: &Nes, vector_addr: u16) -> u16 {
    let memory = nes.bus().borrow();
    let lo = memory.read_cpu_for_debugger(vector_addr);
    let hi = memory.read_cpu_for_debugger(vector_addr.wrapping_add(1));
    u16::from_le_bytes([lo, hi])
}

fn run_to_interrupt_entry(nes: &mut Nes, vector_addr: u16, kind: crate::nes::cpu::InterruptKind) {
    const MAX_STEPS: usize = 2_000_000;

    let target_pc = read_vector_target(nes, vector_addr);
    let mut has_exited_required_interrupt = nes.cpu_ref().current_interrupt() != Some(kind);

    for _step in 0..MAX_STEPS {
        if nes.cpu_ref().is_halted() {
            break;
        }

        nes.run_cpu_tick();

        let current_interrupt = nes.cpu_ref().current_interrupt();
        if current_interrupt != Some(kind) {
            has_exited_required_interrupt = true;
            continue;
        }

        if has_exited_required_interrupt && nes.cpu_ref().pc() == target_pc {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::debugging::snapshot;
    use crate::platform::app_context::AppContext;

    #[test]
    fn test_serialize_debugger_snapshot_json_includes_oam_field() {
        let nes = Nes::new(AppContext::new_with_config(Config::default()));
        let snap = snapshot(&nes);
        let json = serialize_debugger_snapshot_json(&snap);
        assert!(json.contains("\"oam\""), "JSON should include oam field");
    }

    #[test]
    fn test_serialize_debugger_snapshot_json_includes_watch_values_field() {
        let mut wasm = WasmNes::new();
        wasm.debugger_watch_add(0x0010);
        let json = wasm.debugger_snapshot_json();
        assert!(
            json.contains("\"watch_values\""),
            "JSON should include watch_values field"
        );
    }

    #[test]
    fn test_serialize_debugger_snapshot_json_includes_recent_trace_field() {
        let mut wasm = WasmNes::new();
        let json = wasm.debugger_snapshot_json();
        assert!(
            json.contains("\"recent_trace\""),
            "JSON should include recent_trace field"
        );
    }

    #[test]
    fn test_get_expansion_port_reports_playchoice10() {
        let wasm = WasmNes::new();
        wasm.app_context
            .borrow_mut()
            .config_mut()
            .nes
            .expansion_port = crate::nes::console::ExpansionPort::Playchoice10;
        assert_eq!(wasm.get_expansion_port(), "playchoice10");
    }
}
