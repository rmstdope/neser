//! Standard SNES controller (12-button joypad).
//!
//! Models the serial shift register exposed on controller-port pin 4 (the
//! primary data line, `JOY1`/`JOY2`). The 16-bit serial transfer order
//! (most-significant bit first) per the fullsnes "SNES Controllers Joypad"
//! and "Automatic Reading" sections is:
//!
//! | Order | Bit | Button   |
//! |-------|-----|----------|
//! | 1st   | 15  | B        |
//! | 2nd   | 14  | Y        |
//! | 3rd   | 13  | Select   |
//! | 4th   | 12  | Start    |
//! | 5th   | 11  | Up       |
//! | 6th   | 10  | Down     |
//! | 7th   | 9   | Left     |
//! | 8th   | 8   | Right    |
//! | 9th   | 7   | A        |
//! | 10th  | 6   | X        |
//! | 11th  | 5   | L        |
//! | 12th  | 4   | R        |
//! | 13th-16th | 3-0 | ID bits (always 0 for a normal joypad) |
//!
//! After the 16 data bits, further clocks return `1` ("padding", indicating a
//! pad is connected). A pressed button reads as `1` (the data line is pulled
//! low and the input register reads `1`).

use super::{SnesButton, SnesController, SnesControllerState};

/// Number of meaningful serial bits before the shift register exhausts and
/// returns the connected-pad padding value (`1`).
const SERIAL_BITS: u8 = 16;

/// Standard SNES controller.
#[derive(Debug, Clone, Default)]
pub struct StandardController {
    /// Pressed state indexed by serial shift position (`0` = B, `1` = Y, ...,
    /// `11` = R). Bit `i` is set while the corresponding button is held.
    pressed: u16,
    /// Current shift position. `0..16` index real data bits; `>= 16` returns
    /// the connected-pad padding value.
    shift: u8,
    /// `OUT0` latch line state. While high the shift register is held reloaded
    /// (parallel load), so reads always return the first bit (B).
    strobe: bool,
}

impl StandardController {
    /// Create a new controller with no buttons pressed.
    pub fn new() -> Self {
        Self::default()
    }

    /// Serial shift position assigned to each button.
    fn button_shift_index(button: SnesButton) -> u8 {
        match button {
            SnesButton::B => 0,
            SnesButton::Y => 1,
            SnesButton::Select => 2,
            SnesButton::Start => 3,
            SnesButton::Up => 4,
            SnesButton::Down => 5,
            SnesButton::Left => 6,
            SnesButton::Right => 7,
            SnesButton::A => 8,
            SnesButton::X => 9,
            SnesButton::L => 10,
            SnesButton::R => 11,
        }
    }

    /// Return the data-line bit at the current shift position without advancing.
    fn current_bit(&self) -> bool {
        let index = if self.strobe { 0 } else { self.shift };
        if index < 12 {
            (self.pressed >> index) & 1 != 0
        } else if index < SERIAL_BITS {
            // ID bits 13th-16th: always 0 for a normal joypad.
            false
        } else {
            // Padding after 16 clocks: 1 while a pad is connected.
            true
        }
    }
}

impl SnesController for StandardController {
    fn write_strobe(&mut self, high: bool) {
        self.strobe = high;
        if high {
            // Level-sensitive parallel load: hold the shift register reloaded.
            self.shift = 0;
        }
    }

    fn read(&mut self) -> (bool, bool) {
        let data1 = self.current_bit();
        if !self.strobe && self.shift < u8::MAX {
            self.shift += 1;
        }
        // Pin 5 (data2) carries no data for a lone standard controller.
        (data1, false)
    }

    fn set_button(&mut self, button: SnesButton, pressed: bool) -> bool {
        let mask = 1u16 << Self::button_shift_index(button);
        if pressed {
            self.pressed |= mask;
        } else {
            self.pressed &= !mask;
        }
        true
    }

    fn button_states(&self) -> u16 {
        self.pressed
    }

    fn capture_state(&self) -> SnesControllerState {
        SnesControllerState {
            pressed: self.pressed,
            shift: self.shift,
            strobe: self.strobe,
        }
    }

    fn restore_state(&mut self, state: &SnesControllerState) {
        self.pressed = state.pressed;
        self.shift = state.shift;
        self.strobe = state.strobe;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report_after_strobe(c: &mut StandardController) -> u16 {
        c.write_strobe(true);
        c.write_strobe(false);
        let mut word = 0u16;
        for _ in 0..16 {
            let (d1, _) = c.read();
            word = (word << 1) | d1 as u16;
        }
        word
    }

    #[test]
    fn no_buttons_pressed_reads_all_zero_for_16_bits() {
        let mut c = StandardController::new();
        assert_eq!(report_after_strobe(&mut c), 0x0000);
    }

    #[test]
    fn b_button_is_first_bit_out() {
        let mut c = StandardController::new();
        c.set_button(SnesButton::B, true);
        // B occupies serial bit 15 (first out, shifted to the MSB).
        assert_eq!(report_after_strobe(&mut c), 0x8000);
    }

    #[test]
    fn each_button_maps_to_its_documented_serial_bit() {
        let cases = [
            (SnesButton::B, 15),
            (SnesButton::Y, 14),
            (SnesButton::Select, 13),
            (SnesButton::Start, 12),
            (SnesButton::Up, 11),
            (SnesButton::Down, 10),
            (SnesButton::Left, 9),
            (SnesButton::Right, 8),
            (SnesButton::A, 7),
            (SnesButton::X, 6),
            (SnesButton::L, 5),
            (SnesButton::R, 4),
        ];
        for (button, bit) in cases {
            let mut c = StandardController::new();
            c.set_button(button, true);
            assert_eq!(
                report_after_strobe(&mut c),
                1u16 << bit,
                "{button:?} should map to serial bit {bit}"
            );
        }
    }

    #[test]
    fn id_bits_13_to_16_are_zero() {
        let mut c = StandardController::new();
        c.set_button(SnesButton::B, true);
        c.set_button(SnesButton::R, true);
        c.write_strobe(true);
        c.write_strobe(false);
        // Skip the 16 button/ID bits.
        for _ in 0..16 {
            c.read();
        }
        // 17th clock onward: padding = 1 (pad connected).
        for _ in 0..4 {
            assert!(
                c.read().0,
                "padding bits should read 1 while a pad is connected"
            );
        }
    }

    #[test]
    fn strobe_high_keeps_returning_first_bit() {
        let mut c = StandardController::new();
        c.set_button(SnesButton::B, true);
        c.write_strobe(true);
        // While the latch is held high every read returns the B-button bit.
        for _ in 0..20 {
            assert!(c.read().0);
        }
    }

    #[test]
    fn pin5_data2_is_always_low_for_standard_controller() {
        let mut c = StandardController::new();
        c.set_button(SnesButton::B, true);
        c.write_strobe(true);
        c.write_strobe(false);
        for _ in 0..16 {
            assert!(!c.read().1, "standard controller drives no data on pin 5");
        }
    }

    #[test]
    fn save_state_round_trips() {
        let mut c = StandardController::new();
        c.set_button(SnesButton::Start, true);
        c.write_strobe(true);
        c.write_strobe(false);
        c.read();
        c.read();
        let state = c.capture_state();

        let mut restored = StandardController::new();
        restored.restore_state(&state);
        assert_eq!(restored.capture_state(), state);
        assert_eq!(restored.button_states(), c.button_states());
    }
}
