//! SNES input (controller) handling.
//!
//! This module implements the SNES controller ports and the standard 12-button
//! joypad, wired through both access methods documented by fullsnes
//! ("SNES Controllers I/O Ports"):
//!
//! - **Manual serial read** via `$4016`/`$4017` (JOYSER0/1) with the `$4016`
//!   bit-0 strobe ([`InputPorts::write_joywr`]).
//! - **Automatic reading** into `$4218`-`$421F` (JOY1-JOY4), enabled by
//!   `$4200` bit 0 and reported busy via `$4212` bit 0.
//!
//! The bus owns a single [`InputPorts`] and routes the relevant registers to
//! it; the two access methods share each controller's internal shift register
//! so the documented "manual read during auto-joypad corrupts state" behaviour
//! falls out naturally.

mod mouse_controller;
mod multitap;
mod standard_controller;
mod super_scope;

use serde::{Deserialize, Serialize};

pub use mouse_controller::MouseController;
pub use multitap::{Multitap, MultitapState};
pub use standard_controller::StandardController;
pub use super_scope::SuperScopeController;

/// The 12 logical buttons of a standard SNES controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnesButton {
    B,
    Y,
    Select,
    Start,
    Up,
    Down,
    Left,
    Right,
    A,
    X,
    L,
    R,
}

/// Convert a platform button id to a [`SnesButton`].
///
/// Ids extend the platform-wide NES convention with `X` and `Y`:
/// `0=A, 1=B, 2=Select, 3=Start, 4=Up, 5=Down, 6=Left, 7=Right, 8=L, 9=R,
/// 10=X, 11=Y`.
pub fn button_from_id(id: u8) -> Option<SnesButton> {
    Some(match id {
        0 => SnesButton::A,
        1 => SnesButton::B,
        2 => SnesButton::Select,
        3 => SnesButton::Start,
        4 => SnesButton::Up,
        5 => SnesButton::Down,
        6 => SnesButton::Left,
        7 => SnesButton::Right,
        8 => SnesButton::L,
        9 => SnesButton::R,
        10 => SnesButton::X,
        11 => SnesButton::Y,
        _ => return None,
    })
}

/// Persisted state for a single controller's shift register.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct SnesControllerState {
    #[serde(default)]
    pub pressed: u16,
    #[serde(default)]
    pub shift: u8,
    #[serde(default)]
    pub strobe: bool,
    #[serde(default)]
    pub mouse_speed: u8,
    #[serde(default)]
    pub mouse_left_button: bool,
    #[serde(default)]
    pub mouse_right_button: bool,
    #[serde(default)]
    pub mouse_accum_dx: i16,
    #[serde(default)]
    pub mouse_accum_dy: i16,
    #[serde(default)]
    pub mouse_report_dx: i16,
    #[serde(default)]
    pub mouse_report_dy: i16,
    #[serde(default)]
    pub superscope_x: i16,
    #[serde(default)]
    pub superscope_y: i16,
    #[serde(default)]
    pub superscope_trigger: bool,
    #[serde(default)]
    pub superscope_cursor: bool,
    #[serde(default)]
    pub superscope_turbo: bool,
    #[serde(default)]
    pub superscope_pause: bool,
    #[serde(default)]
    pub superscope_offscreen: bool,
    #[serde(default)]
    pub superscope_turbo_enabled: bool,
    #[serde(default)]
    pub superscope_turbo_lock: bool,
    #[serde(default)]
    pub superscope_trigger_output: bool,
    #[serde(default)]
    pub superscope_pause_output: bool,
    #[serde(default)]
    pub superscope_trigger_lock: bool,
    #[serde(default)]
    pub superscope_pause_lock: bool,
    #[serde(default)]
    pub superscope_latched: bool,
}

/// The kind of device plugged into a controller port.
///
/// Selectable per port via `--snes-controller-port1` / `--snes-controller-port2`.
/// [`Standard`](Self::Standard), [`Multitap`](Self::Multitap), and
/// [`Mouse`](Self::Mouse) are implemented; remaining variants currently fall
/// back to a standard controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SnesControllerType {
    #[default]
    Standard,
    Multitap,
    Mouse,
    SuperScope,
}

impl SnesControllerType {
    /// Parse a controller-type config/CLI value (case-insensitive).
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "standard" => Some(Self::Standard),
            "multitap" => Some(Self::Multitap),
            "mouse" => Some(Self::Mouse),
            "superscope" => Some(Self::SuperScope),
            _ => None,
        }
    }

    /// Build the controller device for this type.
    fn build(self) -> Box<dyn SnesController> {
        match self {
            Self::Standard => Box::new(StandardController::new()),
            Self::Multitap => Box::new(Multitap::new()),
            Self::Mouse => Box::new(MouseController::new()),
            Self::SuperScope => Box::new(SuperScopeController::new()),
        }
    }
}

/// Behaviour shared by all devices that plug into a SNES controller port.
pub trait SnesController {
    /// Drive the `OUT0` strobe/latch line shared by both gameports (`$4016`
    /// bit 0). While high the shift register is held reloaded.
    fn write_strobe(&mut self, high: bool);

    /// Update the controller-port PIO select line (`$4201` bit 7).
    fn write_select(&mut self, _high: bool) {}

    /// Return the current serial bit pair `(data1, data2)` exposed on the
    /// port's pin 4 / pin 5 data lines and advance the shift register by one
    /// clock (unless the strobe line is held high).
    fn read(&mut self) -> (bool, bool);

    /// Set a logical button's pressed state. Returns `true` if the device
    /// supports the button.
    fn set_button(&mut self, button: SnesButton, pressed: bool) -> bool;

    /// Set a button on a logical player slot. The default implementation maps
    /// slot 0 to [`Self::set_button`] and ignores the rest.
    fn set_player_button(&mut self, player: u8, button: SnesButton, pressed: bool) -> bool {
        if player == 0 {
            self.set_button(button, pressed)
        } else {
            false
        }
    }

    /// Add relative mouse motion in host-space units.
    fn add_mouse_delta(&mut self, _dx: i16, _dy: i16) -> bool {
        false
    }

    /// Set the left mouse button state.
    fn set_mouse_left_button(&mut self, _pressed: bool) -> bool {
        false
    }

    /// Set the right mouse button state.
    fn set_mouse_right_button(&mut self, _pressed: bool) -> bool {
        false
    }

    /// Set the Super Scope aiming coordinates.
    fn set_superscope_position(&mut self, _x: i16, _y: i16) -> bool {
        false
    }

    /// Set the Super Scope trigger button state.
    fn set_superscope_trigger(&mut self, _pressed: bool) -> bool {
        false
    }

    /// Set the Super Scope cursor button state.
    fn set_superscope_cursor(&mut self, _pressed: bool) -> bool {
        false
    }

    /// Set the Super Scope turbo switch state.
    fn set_superscope_turbo(&mut self, _pressed: bool) -> bool {
        false
    }

    /// Set the Super Scope pause button state.
    fn set_superscope_pause(&mut self, _pressed: bool) -> bool {
        false
    }

    /// Whether this device is a Super Scope.
    fn is_superscope(&self) -> bool {
        false
    }

    /// Whether this device is an SNES mouse.
    fn is_mouse(&self) -> bool {
        false
    }

    /// Return the raw pressed-state mask in serial-bit order (bit 0 = B,
    /// bit 1 = Y, ..., bit 11 = R). Devices without buttons return `0`.
    fn button_states(&self) -> u16 {
        0
    }

    /// Return the 8-bit NES-convention joypad state for a logical player slot.
    fn player_joypad_button_states(&self, player: u8) -> u8 {
        if player == 0 {
            pressed_mask_to_joypad_state(self.button_states())
        } else {
            0
        }
    }

    /// Capture the device's shift-register state for a save-state.
    fn capture_state(&self) -> SnesControllerState;

    /// Restore the device's shift-register state from a save-state.
    fn restore_state(&mut self, state: &SnesControllerState);

    /// Capture extended state for multi-controller devices such as multitaps.
    fn capture_multitap_state(&self) -> Option<MultitapState> {
        let _ = self;
        None
    }

    /// Restore extended state for multi-controller devices such as multitaps.
    fn restore_multitap_state(&mut self, _state: &MultitapState) {}
}

/// Master-clock duration of the auto-joypad busy window (fullsnes: the read
/// "ends 4224 master cycles later", with `$4212` bit 0 set during it).
const AUTO_JOYPAD_BUSY_CYCLES: u32 = 4224;

/// Master clocks between successive auto-joypad bit clocks (16 bits spread over
/// the busy window; fullsnes notes the read advances in ~256-cycle steps).
const AUTO_JOYPAD_BIT_INTERVAL: u32 = AUTO_JOYPAD_BUSY_CYCLES / 16;

/// Persisted state for the whole input subsystem.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct InputPortsState {
    #[serde(default)]
    pub port1_type: SnesControllerType,
    #[serde(default)]
    pub port2_type: SnesControllerType,
    #[serde(default)]
    pub port1: SnesControllerState,
    #[serde(default)]
    pub port2: SnesControllerState,
    #[serde(default)]
    pub port1_multitap: Option<MultitapState>,
    #[serde(default)]
    pub port2_multitap: Option<MultitapState>,
    #[serde(default)]
    pub auto_enable: bool,
    #[serde(default)]
    pub busy_cycles: u32,
    #[serde(default)]
    pub joy: [u16; 4],
    #[serde(default)]
    pub auto_bits_done: u8,
    #[serde(default = "default_wrio")]
    pub wrio: u8,
    #[serde(default)]
    pub strobe: bool,
}

/// The pair of SNES controller ports and the auto-joypad sequencer.
pub struct InputPorts {
    port1_type: SnesControllerType,
    port2_type: SnesControllerType,
    port1: Box<dyn SnesController>,
    port2: Box<dyn SnesController>,
    /// Auto-joypad enable (`$4200` bit 0).
    auto_enable: bool,
    /// Remaining master cycles of the auto-joypad busy window.
    busy_cycles: u32,
    /// Auto-read result registers JOY1-JOY4 (`$4218`-`$421F`). These are shifted
    /// in progressively over the busy window, so a read mid-window sees an
    /// incomplete (and possibly manual-read-corrupted) value.
    joy: [u16; 4],
    /// Number of auto-read bits clocked so far in the current busy window (0-16).
    auto_bits_done: u8,
    /// Last WRIO ($4201) value written by the CPU.
    wrio: u8,
    /// Last value written to `$4016` bit 0 (the `OUT0` strobe).
    strobe: bool,
}

impl Default for InputPorts {
    fn default() -> Self {
        Self::new()
    }
}

impl InputPorts {
    /// Create a pair of ports, each holding a standard controller.
    pub fn new() -> Self {
        Self {
            port1_type: SnesControllerType::Standard,
            port2_type: SnesControllerType::Standard,
            port1: Box::new(StandardController::new()),
            port2: Box::new(StandardController::new()),
            auto_enable: false,
            busy_cycles: 0,
            joy: [0; 4],
            auto_bits_done: 0,
            wrio: default_wrio(),
            strobe: false,
        }
    }

    /// Replace the port devices according to the configured controller types.
    pub fn configure(&mut self, port1: SnesControllerType, port2: SnesControllerType) {
        let resolved_port1 = if port1 == SnesControllerType::Multitap {
            crate::platform::debugging::log_info(
                "SNES multitap on port 1 is not supported; using a standard controller".to_string(),
            );
            SnesControllerType::Standard
        } else {
            port1
        };
        self.port1_type = resolved_port1;
        self.port2_type = port2;
        self.port1 = resolved_port1.build();
        self.port2 = port2.build();
        self.write_wrio(self.wrio);
    }

    /// `$4016` write (JOYWR): drive the `OUT0` strobe line of both gameports.
    pub fn write_joywr(&mut self, value: u8) {
        let strobe = value & 0x01 != 0;
        self.strobe = strobe;
        self.port1.write_strobe(strobe);
        self.port2.write_strobe(strobe);
    }

    /// Write WRIO ($4201) and fan out the per-port select bits:
    /// bit 6 -> port 1 pin 6, bit 7 -> port 2 pin 6.
    pub fn write_wrio(&mut self, value: u8) {
        self.wrio = value;
        self.port1.write_select(value & 0x40 != 0);
        self.port2.write_select(value & 0x80 != 0);
    }

    /// `$4016` read (JOYA): bit 0 = gameport 1 pin 4 (JOY1), bit 1 = gameport 1
    /// pin 5 (JOY3); bits 7-2 are open bus. Reading clocks gameport 1.
    pub fn read_joya(&mut self, open_bus: u8) -> u8 {
        let (d1, d2) = self.port1.read();
        (open_bus & 0xFC) | (d1 as u8) | ((d2 as u8) << 1)
    }

    /// `$4017` read (JOYB): bit 0 = gameport 2 pin 4 (JOY2), bit 1 = gameport 2
    /// pin 5 (JOY4); bits 4-2 are grounded and always read `1`; bits 7-5 are
    /// open bus. Reading clocks gameport 2.
    pub fn read_joyb(&mut self, open_bus: u8) -> u8 {
        let (d1, d2) = self.port2.read();
        (open_bus & 0xE0) | 0x1C | (d1 as u8) | ((d2 as u8) << 1)
    }

    /// Set the auto-joypad enable bit (`$4200` bit 0).
    pub fn set_auto_enable(&mut self, enable: bool) {
        self.auto_enable = enable;
    }

    /// Whether auto-joypad reading is currently enabled.
    pub fn auto_enabled(&self) -> bool {
        self.auto_enable
    }

    /// Whether the auto-joypad busy window is active (`$4212` bit 0).
    pub fn auto_busy(&self) -> bool {
        self.busy_cycles > 0
    }

    /// Read a JOY1-JOY4 auto-read register byte (`$4218`-`$421F`).
    pub fn read_joy_register(&self, offset: u16) -> Option<u8> {
        let index = ((offset - 0x4218) / 2) as usize;
        if index >= self.joy.len() {
            return None;
        }
        let word = self.joy[index];
        Some(if offset & 1 == 0 {
            (word & 0x00FF) as u8
        } else {
            (word >> 8) as u8
        })
    }

    /// Advance the auto-joypad busy window by one master clock, clocking the
    /// controller shift registers one bit at a time as the window elapses.
    pub fn tick(&mut self) {
        if self.busy_cycles == 0 {
            return;
        }
        self.busy_cycles -= 1;
        let elapsed = AUTO_JOYPAD_BUSY_CYCLES - self.busy_cycles;
        while (self.auto_bits_done as u32) < 16
            && elapsed >= (self.auto_bits_done as u32 + 1) * AUTO_JOYPAD_BIT_INTERVAL
        {
            self.shift_auto_bit();
            self.auto_bits_done += 1;
        }
    }

    /// Begin an auto-joypad read at the start of VBlank: latch (parallel load)
    /// then release both ports, reset the result registers, and start the busy
    /// window. The 16 data bits are clocked into JOY1-JOY4 progressively over
    /// the window (see [`Self::tick`]) using the same shift registers as manual
    /// `$4016`/`$4017` reads, so a manual read mid-window corrupts the result
    /// exactly as on hardware.
    pub fn trigger_auto_read(&mut self) {
        if !self.auto_enable {
            return;
        }
        self.port1.write_strobe(true);
        self.port2.write_strobe(true);
        self.port1.write_strobe(false);
        self.port2.write_strobe(false);
        self.strobe = false;
        self.joy = [0; 4];
        self.auto_bits_done = 0;
        self.busy_cycles = AUTO_JOYPAD_BUSY_CYCLES;
    }

    /// Clock one bit out of both ports and shift it into the JOY registers
    /// (MSB first). JOY1/JOY2 take pin-4 data; JOY3/JOY4 take pin-5 data.
    fn shift_auto_bit(&mut self) {
        let (p1d1, p1d2) = self.port1.read();
        let (p2d1, p2d2) = self.port2.read();
        self.joy[0] = (self.joy[0] << 1) | p1d1 as u16;
        self.joy[1] = (self.joy[1] << 1) | p2d1 as u16;
        self.joy[2] = (self.joy[2] << 1) | p1d2 as u16;
        self.joy[3] = (self.joy[3] << 1) | p2d2 as u16;
    }

    /// Set a single button on the given port's device.
    pub fn set_button(&mut self, port: u8, button: SnesButton, pressed: bool) {
        match port {
            0 => {
                let _ = self.port1.set_player_button(0, button, pressed);
            }
            1..=4 => {
                let _ = self.port2.set_player_button(port - 1, button, pressed);
            }
            _ => {}
        }
    }

    /// Bulk-set the 8 NES-convention buttons (A, B, Select, Start, Up, Down,
    /// Left, Right) on the given port, preserving X/Y/L/R.
    pub fn set_joypad_button_states(&mut self, port: u8, state: u8) {
        const BUTTONS: [SnesButton; 8] = [
            SnesButton::A,
            SnesButton::B,
            SnesButton::Select,
            SnesButton::Start,
            SnesButton::Up,
            SnesButton::Down,
            SnesButton::Left,
            SnesButton::Right,
        ];
        let apply = |device: &mut dyn SnesController, player: u8| {
            for (bit, button) in BUTTONS.into_iter().enumerate() {
                let _ = device.set_player_button(player, button, state & (1 << bit) != 0);
            }
        };
        match port {
            0 => apply(self.port1.as_mut(), 0),
            1..=4 => apply(self.port2.as_mut(), port - 1),
            _ => {}
        }
    }

    /// Add relative mouse motion to the configured device on the given port.
    pub fn add_mouse_delta(&mut self, port: u8, dx: i16, dy: i16) {
        match port {
            0 => {
                let _ = self.port1.add_mouse_delta(dx, dy);
            }
            1..=4 => {
                let _ = self.port2.add_mouse_delta(dx, dy);
            }
            _ => {}
        }
    }

    /// Set left mouse button state on the configured device on the given port.
    pub fn set_mouse_left_button(&mut self, port: u8, pressed: bool) {
        match port {
            0 => {
                let _ = self.port1.set_mouse_left_button(pressed);
            }
            1..=4 => {
                let _ = self.port2.set_mouse_left_button(pressed);
            }
            _ => {}
        }
    }

    /// Set right mouse button state on the configured device on the given port.
    pub fn set_mouse_right_button(&mut self, port: u8, pressed: bool) {
        match port {
            0 => {
                let _ = self.port1.set_mouse_right_button(pressed);
            }
            1..=4 => {
                let _ = self.port2.set_mouse_right_button(pressed);
            }
            _ => {}
        }
    }

    /// Set Super Scope aiming coordinates on the configured device.
    pub fn set_superscope_position(&mut self, port: u8, x: i16, y: i16) {
        match port {
            0 => {
                let _ = self.port1.set_superscope_position(x, y);
            }
            1..=4 => {
                let _ = self.port2.set_superscope_position(x, y);
            }
            _ => {}
        }
    }

    /// Set Super Scope trigger button state on the configured device.
    pub fn set_superscope_trigger(&mut self, port: u8, pressed: bool) {
        match port {
            0 => {
                let _ = self.port1.set_superscope_trigger(pressed);
            }
            1..=4 => {
                let _ = self.port2.set_superscope_trigger(pressed);
            }
            _ => {}
        }
    }

    /// Set Super Scope cursor button state on the configured device.
    pub fn set_superscope_cursor(&mut self, port: u8, pressed: bool) {
        match port {
            0 => {
                let _ = self.port1.set_superscope_cursor(pressed);
            }
            1..=4 => {
                let _ = self.port2.set_superscope_cursor(pressed);
            }
            _ => {}
        }
    }

    /// Set Super Scope turbo switch state on the configured device.
    pub fn set_superscope_turbo(&mut self, port: u8, pressed: bool) {
        match port {
            0 => {
                let _ = self.port1.set_superscope_turbo(pressed);
            }
            1..=4 => {
                let _ = self.port2.set_superscope_turbo(pressed);
            }
            _ => {}
        }
    }

    /// Set Super Scope pause button state on the configured device.
    pub fn set_superscope_pause(&mut self, port: u8, pressed: bool) {
        match port {
            0 => {
                let _ = self.port1.set_superscope_pause(pressed);
            }
            1..=4 => {
                let _ = self.port2.set_superscope_pause(pressed);
            }
            _ => {}
        }
    }

    /// Returns true if any controller port currently hosts an SNES mouse.
    pub fn has_mouse(&self) -> bool {
        self.port1.is_mouse() || self.port2.is_mouse()
    }

    /// Returns true if any controller port currently hosts a Super Scope.
    pub fn has_superscope(&self) -> bool {
        self.port1.is_superscope() || self.port2.is_superscope()
    }

    /// Returns true if the given physical SNES port hosts a mouse.
    pub fn has_mouse_on_port(&self, port: u8) -> bool {
        match port {
            0 => self.port1.is_mouse(),
            1 => self.port2.is_mouse(),
            _ => false,
        }
    }

    /// Returns true if the given physical SNES port currently hosts a Super Scope.
    pub fn has_superscope_on_port(&self, port: u8) -> bool {
        match port {
            0 => self.port1.is_superscope(),
            1 => self.port2.is_superscope(),
            _ => false,
        }
    }

    /// Returns true if the given physical SNES port currently hosts a multitap.
    pub fn is_multitap_on_port(&self, port: u8) -> bool {
        match port {
            0 => self.port1_type == SnesControllerType::Multitap,
            1 => self.port2_type == SnesControllerType::Multitap,
            _ => false,
        }
    }

    /// Return the 8 NES-convention button states for the given port.
    pub fn joypad_button_states(&self, port: u8) -> u8 {
        match port {
            0 => self.port1.player_joypad_button_states(0),
            1..=4 => self.port2.player_joypad_button_states(port - 1),
            _ => 0,
        }
    }

    /// Capture the input subsystem state for a save-state.
    pub fn capture_state(&self) -> InputPortsState {
        InputPortsState {
            port1_type: self.port1_type,
            port2_type: self.port2_type,
            port1: self.port1.capture_state(),
            port2: self.port2.capture_state(),
            port1_multitap: self.port1.capture_multitap_state(),
            port2_multitap: self.port2.capture_multitap_state(),
            auto_enable: self.auto_enable,
            busy_cycles: self.busy_cycles,
            joy: self.joy,
            auto_bits_done: self.auto_bits_done,
            wrio: self.wrio,
            strobe: self.strobe,
        }
    }

    /// Restore the input subsystem state from a save-state.
    pub fn restore_state(&mut self, state: &InputPortsState) {
        self.configure(state.port1_type, state.port2_type);
        self.port1.restore_state(&state.port1);
        self.port2.restore_state(&state.port2);
        if let Some(multitap_state) = &state.port1_multitap {
            self.port1.restore_multitap_state(multitap_state);
        }
        if let Some(multitap_state) = &state.port2_multitap {
            self.port2.restore_multitap_state(multitap_state);
        }
        self.auto_enable = state.auto_enable;
        self.busy_cycles = state.busy_cycles;
        self.joy = state.joy;
        self.auto_bits_done = state.auto_bits_done;
        self.write_wrio(state.wrio);
        self.strobe = state.strobe;
    }
}

fn default_wrio() -> u8 {
    0xFF
}

fn pressed_mask_to_joypad_state(pressed: u16) -> u8 {
    let bit = |i: u8| ((pressed >> i) & 1) as u8;
    bit(8)            // A   -> bit 0
        | (bit(0) << 1) // B   -> bit 1
        | (bit(2) << 2) // Sel -> bit 2
        | (bit(3) << 3) // Sta -> bit 3
        | (bit(4) << 4) // Up  -> bit 4
        | (bit(5) << 5) // Dn  -> bit 5
        | (bit(6) << 6) // Lf  -> bit 6
        | (bit(7) << 7) // Rt  -> bit 7
}

#[cfg(test)]
mod tests {
    use super::mouse_controller::MouseController;
    use super::*;

    #[test]
    fn button_id_mapping_covers_all_twelve_buttons() {
        let ids = [
            (0, SnesButton::A),
            (1, SnesButton::B),
            (2, SnesButton::Select),
            (3, SnesButton::Start),
            (4, SnesButton::Up),
            (5, SnesButton::Down),
            (6, SnesButton::Left),
            (7, SnesButton::Right),
            (8, SnesButton::L),
            (9, SnesButton::R),
            (10, SnesButton::X),
            (11, SnesButton::Y),
        ];
        for (id, button) in ids {
            assert_eq!(button_from_id(id), Some(button));
        }
        assert_eq!(button_from_id(12), None);
    }

    #[test]
    fn auto_read_disabled_does_not_start_busy_window() {
        let mut ports = InputPorts::new();
        ports.trigger_auto_read();
        assert!(!ports.auto_busy());
    }

    #[test]
    fn auto_read_populates_joy1_after_busy_window() {
        let mut ports = InputPorts::new();
        ports.set_auto_enable(true);
        ports.set_button(0, SnesButton::B, true);
        ports.set_button(0, SnesButton::Start, true);
        ports.trigger_auto_read();

        assert!(ports.auto_busy(), "busy flag set immediately");
        // Before any bits are clocked, JOY1 reads as the freshly-reset value.
        assert_eq!(ports.read_joy_register(0x4218), Some(0x00));

        for _ in 0..AUTO_JOYPAD_BUSY_CYCLES {
            ports.tick();
        }
        assert!(!ports.auto_busy(), "busy flag cleared after the window");

        // JOY1: B = bit 15, Start = bit 12 -> 0x9000.
        assert_eq!(ports.read_joy_register(0x4218), Some(0x00)); // low byte
        assert_eq!(ports.read_joy_register(0x4219), Some(0x90)); // high byte
    }

    #[test]
    fn manual_read_during_auto_window_corrupts_the_result() {
        // Clean reference read (no interference).
        let mut clean = InputPorts::new();
        clean.set_auto_enable(true);
        clean.set_button(0, SnesButton::A, true);
        clean.trigger_auto_read();
        for _ in 0..AUTO_JOYPAD_BUSY_CYCLES {
            clean.tick();
        }
        let clean_joy1 = (clean.read_joy_register(0x4219).unwrap() as u16) << 8
            | clean.read_joy_register(0x4218).unwrap() as u16;
        assert_eq!(clean_joy1, 0x0080, "A = serial bit 7");

        // Same input, but a manual $4016 read steals a clock from port 1 partway
        // through the busy window, desyncing the shared shift register.
        let mut corrupted = InputPorts::new();
        corrupted.set_auto_enable(true);
        corrupted.set_button(0, SnesButton::A, true);
        corrupted.trigger_auto_read();
        for _ in 0..(AUTO_JOYPAD_BUSY_CYCLES / 4) {
            corrupted.tick();
        }
        corrupted.read_joya(0x00); // stolen clock
        for _ in 0..(AUTO_JOYPAD_BUSY_CYCLES - AUTO_JOYPAD_BUSY_CYCLES / 4) {
            corrupted.tick();
        }
        let corrupted_joy1 = (corrupted.read_joy_register(0x4219).unwrap() as u16) << 8
            | corrupted.read_joy_register(0x4218).unwrap() as u16;
        assert_ne!(
            corrupted_joy1, clean_joy1,
            "a manual read during the busy window corrupts the auto-read result"
        );
    }

    #[test]
    fn auto_read_uses_correct_port_to_joy_mapping() {
        let mut ports = InputPorts::new();
        ports.set_auto_enable(true);
        ports.set_button(1, SnesButton::A, true); // port 2 -> JOY2
        ports.trigger_auto_read();
        for _ in 0..AUTO_JOYPAD_BUSY_CYCLES {
            ports.tick();
        }
        // A = serial bit 7 (9th out) -> JOY word bit 7 = 0x0080.
        assert_eq!(ports.read_joy_register(0x421A), Some(0x80)); // JOY2 low
        assert_eq!(ports.read_joy_register(0x421B), Some(0x00)); // JOY2 high
        assert_eq!(ports.read_joy_register(0x4218), Some(0x00)); // JOY1 untouched
    }

    #[test]
    fn manual_serial_read_agrees_with_auto_read() {
        let mut ports = InputPorts::new();
        ports.set_button(0, SnesButton::B, true);
        ports.set_button(0, SnesButton::Down, true);
        ports.set_button(0, SnesButton::R, true);

        // Auto path.
        ports.set_auto_enable(true);
        ports.trigger_auto_read();
        for _ in 0..AUTO_JOYPAD_BUSY_CYCLES {
            ports.tick();
        }
        let auto = (ports.read_joy_register(0x4219).unwrap() as u16) << 8
            | ports.read_joy_register(0x4218).unwrap() as u16;

        // Manual path: strobe then read 16 bits from $4016 bit 0.
        ports.write_joywr(1);
        ports.write_joywr(0);
        let mut manual = 0u16;
        for _ in 0..16 {
            let bit = ports.read_joya(0x00) & 1;
            manual = (manual << 1) | bit as u16;
        }
        assert_eq!(manual, auto);
    }

    #[test]
    fn joyb_grounded_bits_2_to_4_read_one() {
        let mut ports = InputPorts::new();
        let value = ports.read_joyb(0x00);
        assert_eq!(value & 0x1C, 0x1C, "bits 2-4 are grounded and read 1");
    }

    #[test]
    fn joya_preserves_open_bus_upper_bits() {
        let mut ports = InputPorts::new();
        let value = ports.read_joya(0xFC);
        assert_eq!(value & 0xFC, 0xFC, "bits 7-2 reflect open bus");
    }

    #[test]
    fn joyb_preserves_open_bus_bits_7_to_5() {
        let mut ports = InputPorts::new();
        let value = ports.read_joyb(0xE0);
        assert_eq!(value & 0xE0, 0xE0, "bits 7-5 reflect open bus");
    }

    #[test]
    fn bulk_state_round_trips_eight_buttons_and_preserves_others() {
        let mut ports = InputPorts::new();
        ports.set_button(0, SnesButton::X, true);
        ports.set_button(0, SnesButton::L, true);
        ports.set_joypad_button_states(0, 0b1010_0101);
        assert_eq!(ports.joypad_button_states(0), 0b1010_0101);
        // X and L preserved (visible via auto-read bits 6 and 5).
        ports.set_auto_enable(true);
        ports.trigger_auto_read();
        for _ in 0..AUTO_JOYPAD_BUSY_CYCLES {
            ports.tick();
        }
        let joy1 = (ports.read_joy_register(0x4219).unwrap() as u16) << 8
            | ports.read_joy_register(0x4218).unwrap() as u16;
        assert_ne!(joy1 & (1 << 6), 0, "X preserved");
        assert_ne!(joy1 & (1 << 5), 0, "L preserved");
    }

    #[test]
    fn input_state_round_trips() {
        let mut ports = InputPorts::new();
        ports.set_auto_enable(true);
        ports.set_button(0, SnesButton::Y, true);
        ports.write_wrio(0x40);
        ports.trigger_auto_read();
        ports.tick();
        let state = ports.capture_state();

        let mut restored = InputPorts::new();
        restored.restore_state(&state);
        assert_eq!(restored.capture_state(), state);
    }

    #[test]
    fn superscope_controller_type_builds_superscope_device() {
        let mut ports = InputPorts::new();
        ports.configure(SnesControllerType::SuperScope, SnesControllerType::Standard);
        assert!(ports.has_superscope_on_port(0));
    }

    #[test]
    fn superscope_state_round_trips() {
        let mut ports = InputPorts::new();
        ports.configure(SnesControllerType::SuperScope, SnesControllerType::Standard);
        ports.set_superscope_position(0, 42, 84);
        ports.set_superscope_trigger(0, true);
        ports.set_superscope_cursor(0, true);
        ports.set_superscope_turbo(0, true);
        ports.set_superscope_pause(0, true);

        let state = ports.capture_state();

        let mut restored = InputPorts::new();
        restored.restore_state(&state);
        assert_eq!(restored.capture_state(), state);
    }

    #[test]
    fn configuring_multitap_on_port1_falls_back_to_standard() {
        let mut ports = InputPorts::new();
        ports.configure(SnesControllerType::Multitap, SnesControllerType::Standard);
        let state = ports.capture_state();
        assert_eq!(state.port1_type, SnesControllerType::Standard);
        assert_eq!(state.port2_type, SnesControllerType::Standard);
    }

    #[test]
    fn mouse_controller_type_builds_mouse_device() {
        let mut ports = InputPorts::new();
        ports.configure(SnesControllerType::Mouse, SnesControllerType::Standard);
        assert!(ports.has_mouse());
    }

    #[test]
    fn mouse_relative_delta_and_buttons_affect_serial_report() {
        let mut mouse = MouseController::new();
        mouse.set_mouse_left_button(true);
        mouse.set_mouse_right_button(true);
        mouse.add_mouse_delta(-3, 4);
        mouse.write_strobe(true);
        mouse.write_strobe(false);

        let mut packet = [0u8; 4];
        for byte in &mut packet {
            for _ in 0..8 {
                let (bit, _) = mouse.read();
                *byte = (*byte << 1) | u8::from(bit);
            }
        }

        assert_eq!(packet[1], 0xC1, "header/button byte");
        assert_eq!(packet[2], 0x04, "vertical byte");
        assert_eq!(packet[3], 0x83, "horizontal byte");
    }

    #[test]
    fn mouse_speed_cycles_on_reads_while_strobe_high() {
        let mut mouse = MouseController::new();

        mouse.write_strobe(true);
        let _ = mouse.read();
        mouse.write_strobe(false);
        let slow = mouse.capture_state();
        assert_eq!(slow.mouse_speed, 1);

        mouse.write_strobe(true);
        let _ = mouse.read();
        mouse.write_strobe(false);
        let fast = mouse.capture_state();
        assert_eq!(fast.mouse_speed, 2);

        mouse.write_strobe(true);
        let _ = mouse.read();
        mouse.write_strobe(false);
        let normal = mouse.capture_state();
        assert_eq!(normal.mouse_speed, 0);
    }
}
