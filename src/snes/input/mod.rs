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

mod standard_controller;

use serde::{Deserialize, Serialize};

pub use standard_controller::StandardController;

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
}

/// The kind of device plugged into a controller port.
///
/// Selectable per port via `--snes-controller-port1` / `--snes-controller-port2`.
/// Only [`Standard`](Self::Standard) is implemented today; the remaining
/// variants are placeholders for the peripheral sub-issues and currently fall
/// back to a standard controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
            other => {
                crate::platform::debugging::log_info(format!(
                    "SNES controller type {other:?} is not yet implemented; \
                     using a standard controller"
                ));
                Box::new(StandardController::new())
            }
        }
    }
}

/// Behaviour shared by all devices that plug into a SNES controller port.
pub trait SnesController {
    /// Drive the `OUT0` strobe/latch line shared by both gameports (`$4016`
    /// bit 0). While high the shift register is held reloaded.
    fn write_strobe(&mut self, high: bool);

    /// Return the current serial bit pair `(data1, data2)` exposed on the
    /// port's pin 4 / pin 5 data lines and advance the shift register by one
    /// clock (unless the strobe line is held high).
    fn read(&mut self) -> (bool, bool);

    /// Set a logical button's pressed state. Returns `true` if the device
    /// supports the button.
    fn set_button(&mut self, button: SnesButton, pressed: bool) -> bool;

    /// Return the raw pressed-state mask in serial-bit order (bit 0 = B,
    /// bit 1 = Y, ..., bit 11 = R). Devices without buttons return `0`.
    fn button_states(&self) -> u16 {
        0
    }

    /// Capture the device's shift-register state for a save-state.
    fn capture_state(&self) -> SnesControllerState;

    /// Restore the device's shift-register state from a save-state.
    fn restore_state(&mut self, state: &SnesControllerState);
}

/// Master-clock duration of the auto-joypad busy window (fullsnes: the read
/// "ends 4224 master cycles later", with `$4212` bit 0 set during it).
const AUTO_JOYPAD_BUSY_CYCLES: u32 = 4224;

/// Persisted state for the whole input subsystem.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct InputPortsState {
    #[serde(default)]
    pub port1: SnesControllerState,
    #[serde(default)]
    pub port2: SnesControllerState,
    #[serde(default)]
    pub auto_enable: bool,
    #[serde(default)]
    pub busy_cycles: u32,
    #[serde(default)]
    pub joy: [u16; 4],
    #[serde(default)]
    pub pending: [u16; 4],
    #[serde(default)]
    pub strobe: bool,
}

/// The pair of SNES controller ports and the auto-joypad sequencer.
pub struct InputPorts {
    port1: Box<dyn SnesController>,
    port2: Box<dyn SnesController>,
    /// Auto-joypad enable (`$4200` bit 0).
    auto_enable: bool,
    /// Remaining master cycles of the auto-joypad busy window.
    busy_cycles: u32,
    /// Committed auto-read results: JOY1-JOY4 (`$4218`-`$421F`).
    joy: [u16; 4],
    /// Values captured at the start of the busy window, committed to [`Self::joy`]
    /// when the window ends.
    pending: [u16; 4],
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
            port1: Box::new(StandardController::new()),
            port2: Box::new(StandardController::new()),
            auto_enable: false,
            busy_cycles: 0,
            joy: [0; 4],
            pending: [0; 4],
            strobe: false,
        }
    }

    /// Replace the port devices according to the configured controller types.
    pub fn configure(&mut self, port1: SnesControllerType, port2: SnesControllerType) {
        self.port1 = port1.build();
        self.port2 = port2.build();
    }

    fn port_mut(&mut self, port: u8) -> Option<&mut dyn SnesController> {
        match port {
            0 => Some(self.port1.as_mut()),
            1 => Some(self.port2.as_mut()),
            _ => None,
        }
    }

    fn port(&self, port: u8) -> Option<&dyn SnesController> {
        match port {
            0 => Some(self.port1.as_ref()),
            1 => Some(self.port2.as_ref()),
            _ => None,
        }
    }

    /// `$4016` write (JOYWR): drive the `OUT0` strobe line of both gameports.
    pub fn write_joywr(&mut self, value: u8) {
        let strobe = value & 0x01 != 0;
        self.strobe = strobe;
        self.port1.write_strobe(strobe);
        self.port2.write_strobe(strobe);
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

    /// Advance the auto-joypad busy window by one master clock.
    pub fn tick(&mut self) {
        if self.busy_cycles > 0 {
            self.busy_cycles -= 1;
            if self.busy_cycles == 0 {
                self.joy = self.pending;
            }
        }
    }

    /// Begin an auto-joypad read at the start of VBlank. Captures the latched
    /// controller data into [`Self::pending`] and starts the busy window; the
    /// data becomes visible in JOY1-JOY4 when the window ends.
    pub fn trigger_auto_read(&mut self) {
        if !self.auto_enable {
            return;
        }
        let (joy1, joy3) = Self::latch_and_shift(self.port1.as_mut());
        let (joy2, joy4) = Self::latch_and_shift(self.port2.as_mut());
        self.pending = [joy1, joy2, joy3, joy4];
        self.busy_cycles = AUTO_JOYPAD_BUSY_CYCLES;
    }

    /// Strobe a port and shift out 16 bits from both data lines, MSB first.
    fn latch_and_shift(port: &mut dyn SnesController) -> (u16, u16) {
        port.write_strobe(true);
        port.write_strobe(false);
        let mut data1 = 0u16;
        let mut data2 = 0u16;
        for _ in 0..16 {
            let (d1, d2) = port.read();
            data1 = (data1 << 1) | d1 as u16;
            data2 = (data2 << 1) | d2 as u16;
        }
        (data1, data2)
    }

    /// Set a single button on the given port's device.
    pub fn set_button(&mut self, port: u8, button: SnesButton, pressed: bool) {
        if let Some(device) = self.port_mut(port) {
            device.set_button(button, pressed);
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
        let Some(device) = self.port_mut(port) else {
            return;
        };
        for (bit, button) in BUTTONS.into_iter().enumerate() {
            device.set_button(button, state & (1 << bit) != 0);
        }
    }

    /// Return the 8 NES-convention button states for the given port.
    pub fn joypad_button_states(&self, port: u8) -> u8 {
        let Some(device) = self.port(port) else {
            return 0;
        };
        let pressed = device.button_states();
        // serial-bit order -> NES-convention byte order.
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

    /// Capture the input subsystem state for a save-state.
    pub fn capture_state(&self) -> InputPortsState {
        InputPortsState {
            port1: self.port1.capture_state(),
            port2: self.port2.capture_state(),
            auto_enable: self.auto_enable,
            busy_cycles: self.busy_cycles,
            joy: self.joy,
            pending: self.pending,
            strobe: self.strobe,
        }
    }

    /// Restore the input subsystem state from a save-state.
    pub fn restore_state(&mut self, state: &InputPortsState) {
        self.port1.restore_state(&state.port1);
        self.port2.restore_state(&state.port2);
        self.auto_enable = state.auto_enable;
        self.busy_cycles = state.busy_cycles;
        self.joy = state.joy;
        self.pending = state.pending;
        self.strobe = state.strobe;
    }
}

#[cfg(test)]
mod tests {
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
        // JOY1 still holds the previous (empty) value during the busy window.
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
        ports.trigger_auto_read();
        ports.tick();
        let state = ports.capture_state();

        let mut restored = InputPorts::new();
        restored.restore_state(&state);
        assert_eq!(restored.capture_state(), state);
    }
}
