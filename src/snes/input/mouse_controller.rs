//! SNES Mouse controller.
//!
//! Serial output format follows the SNES mouse protocol:
//! - first 16 bits: fixed signature/header
//! - next bits: right button, left button, speed bits, Y sign, Y magnitude,
//!   X sign, X magnitude
//! - then ones for open-bus style tail bits

use super::{SnesController, SnesControllerState};

const REPORT_BITS: usize = 32;
const MAGNITUDE_MAX: i16 = 127;

const SPEED_NORMAL: u8 = 0;
const SPEED_SLOW: u8 = 1;
const SPEED_FAST: u8 = 2;
const SPEED_WRAP: u8 = 3;

const HEADER_MASK: u32 = 0x0001;
const REPORT_HEADER: u32 = 0x0001;
const RIGHT_BUTTON_BIT: usize = 16;
const LEFT_BUTTON_BIT: usize = 17;
const SPEED_BIT_0: usize = 18;
const SPEED_BIT_1: usize = 19;
const Y_SIGN_BIT: usize = 20;
const Y_MAG_START: usize = 21;
const X_SIGN_BIT: usize = 28;
const X_MAG_START: usize = 29;

#[derive(Debug, Clone)]
pub struct MouseController {
    speed: u8,
    left_button: bool,
    right_button: bool,
    accum_dx: i16,
    accum_dy: i16,
    report_dx: i16,
    report_dy: i16,
    shift_index: usize,
    strobe: bool,
}

impl Default for MouseController {
    fn default() -> Self {
        Self::new()
    }
}

impl MouseController {
    pub fn new() -> Self {
        Self {
            speed: SPEED_NORMAL,
            left_button: false,
            right_button: false,
            accum_dx: 0,
            accum_dy: 0,
            report_dx: 0,
            report_dy: 0,
            shift_index: 0,
            strobe: false,
        }
    }

    fn clamp_i16(v: i16, lo: i16, hi: i16) -> i16 {
        v.clamp(lo, hi)
    }

    fn sign_and_magnitude(value: i16) -> (bool, u8) {
        if value < 0 {
            (true, (-value) as u8)
        } else {
            (false, value as u8)
        }
    }

    fn add_clamped_with_remainder(accum: &mut i16, delta: i16) {
        let next = *accum as i32 + delta as i32;
        let clamped = next.clamp(-(MAGNITUDE_MAX as i32), MAGNITUDE_MAX as i32) as i16;
        *accum = clamped;
    }

    fn latch_report(&mut self) {
        self.report_dx = Self::clamp_i16(self.accum_dx, -MAGNITUDE_MAX, MAGNITUDE_MAX);
        self.report_dy = Self::clamp_i16(self.accum_dy, -MAGNITUDE_MAX, MAGNITUDE_MAX);
        self.accum_dx = 0;
        self.accum_dy = 0;
        self.shift_index = 0;
    }

    fn cycle_speed(&mut self) {
        self.speed = match self.speed {
            SPEED_NORMAL => SPEED_SLOW,
            SPEED_SLOW => SPEED_FAST,
            SPEED_FAST => SPEED_NORMAL,
            _ => SPEED_NORMAL,
        };
    }

    fn build_report_word(&self) -> u32 {
        let mut word = REPORT_HEADER & HEADER_MASK;

        if self.right_button {
            word |= 1 << RIGHT_BUTTON_BIT;
        }
        if self.left_button {
            word |= 1 << LEFT_BUTTON_BIT;
        }
        if self.speed & 0x01 != 0 {
            word |= 1 << SPEED_BIT_0;
        }
        if self.speed & 0x02 != 0 {
            word |= 1 << SPEED_BIT_1;
        }

        let (y_sign, y_mag) = Self::sign_and_magnitude(self.report_dy);
        if y_sign {
            word |= 1 << Y_SIGN_BIT;
        }
        word |= ((y_mag as u32) & 0x7F) << Y_MAG_START;

        let (x_sign, x_mag) = Self::sign_and_magnitude(self.report_dx);
        if x_sign {
            word |= 1 << X_SIGN_BIT;
        }
        word |= ((x_mag as u32) & 0x7F) << X_MAG_START;

        word
    }
}

impl SnesController for MouseController {
    fn write_strobe(&mut self, high: bool) {
        let prev = self.strobe;
        self.strobe = high;

        if high {
            if !prev {
                self.cycle_speed();
            }
            self.shift_index = 0;
            return;
        }

        if prev {
            self.latch_report();
        }
    }

    fn read(&mut self) -> (bool, bool) {
        let report = self.build_report_word();
        let bit = if self.shift_index < REPORT_BITS {
            ((report >> self.shift_index) & 1) != 0
        } else {
            true
        };

        if !self.strobe {
            self.shift_index = self.shift_index.saturating_add(1);
        }

        (bit, false)
    }

    fn set_button(&mut self, _button: super::SnesButton, _pressed: bool) -> bool {
        false
    }

    fn add_mouse_delta(&mut self, dx: i16, dy: i16) -> bool {
        Self::add_clamped_with_remainder(&mut self.accum_dx, dx);
        Self::add_clamped_with_remainder(&mut self.accum_dy, dy);
        true
    }

    fn set_mouse_left_button(&mut self, pressed: bool) -> bool {
        self.left_button = pressed;
        true
    }

    fn set_mouse_right_button(&mut self, pressed: bool) -> bool {
        self.right_button = pressed;
        true
    }

    fn is_mouse(&self) -> bool {
        true
    }

    fn capture_state(&self) -> SnesControllerState {
        SnesControllerState {
            pressed: 0,
            shift: 0,
            strobe: self.strobe,
            mouse_speed: self.speed,
            mouse_left_button: self.left_button,
            mouse_right_button: self.right_button,
            mouse_accum_dx: self.accum_dx,
            mouse_accum_dy: self.accum_dy,
            mouse_report_dx: self.report_dx,
            mouse_report_dy: self.report_dy,
        }
    }

    fn restore_state(&mut self, state: &SnesControllerState) {
        self.speed = if state.mouse_speed == SPEED_WRAP {
            SPEED_NORMAL
        } else {
            state.mouse_speed
        };
        self.left_button = state.mouse_left_button;
        self.right_button = state.mouse_right_button;
        self.accum_dx = Self::clamp_i16(state.mouse_accum_dx, -MAGNITUDE_MAX, MAGNITUDE_MAX);
        self.accum_dy = Self::clamp_i16(state.mouse_accum_dy, -MAGNITUDE_MAX, MAGNITUDE_MAX);
        self.report_dx = Self::clamp_i16(state.mouse_report_dx, -MAGNITUDE_MAX, MAGNITUDE_MAX);
        self.report_dy = Self::clamp_i16(state.mouse_report_dy, -MAGNITUDE_MAX, MAGNITUDE_MAX);
        self.strobe = state.strobe;
        self.shift_index = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_bits(mouse: &mut MouseController, count: usize) -> u32 {
        let mut out = 0u32;
        for i in 0..count {
            let (bit, _io) = mouse.read();
            if bit {
                out |= 1u32 << i;
            }
        }
        out
    }

    #[test]
    fn default_speed_is_normal_after_first_latch() {
        let mut mouse = MouseController::new();
        mouse.write_strobe(false);
        let report = read_bits(&mut mouse, 32);
        assert_eq!((report >> SPEED_BIT_0) & 1, 0);
        assert_eq!((report >> SPEED_BIT_1) & 1, 0);
    }

    #[test]
    fn strobe_high_cycles_speed_mode() {
        let mut mouse = MouseController::new();

        mouse.write_strobe(true);
        mouse.write_strobe(false);
        let slow = read_bits(&mut mouse, 32);
        assert_eq!((slow >> SPEED_BIT_0) & 1, 1);
        assert_eq!((slow >> SPEED_BIT_1) & 1, 0);

        mouse.write_strobe(true);
        mouse.write_strobe(false);
        let fast = read_bits(&mut mouse, 32);
        assert_eq!((fast >> SPEED_BIT_0) & 1, 0);
        assert_eq!((fast >> SPEED_BIT_1) & 1, 1);

        mouse.write_strobe(true);
        mouse.write_strobe(false);
        let normal = read_bits(&mut mouse, 32);
        assert_eq!((normal >> SPEED_BIT_0) & 1, 0);
        assert_eq!((normal >> SPEED_BIT_1) & 1, 0);
    }

    #[test]
    fn report_contains_buttons_and_signed_magnitudes() {
        let mut mouse = MouseController::new();
        mouse.set_mouse_left_button(true);
        mouse.set_mouse_right_button(true);
        mouse.add_mouse_delta(-5, 6);

        mouse.write_strobe(false);
        let report = read_bits(&mut mouse, 32);

        assert_eq!((report >> RIGHT_BUTTON_BIT) & 1, 1);
        assert_eq!((report >> LEFT_BUTTON_BIT) & 1, 1);

        assert_eq!((report >> Y_SIGN_BIT) & 1, 0);
        assert_eq!((report >> Y_MAG_START) & 0x7F, 6);

        assert_eq!((report >> X_SIGN_BIT) & 1, 1);
        assert_eq!((report >> X_MAG_START) & 0x7F, 5);
    }

    #[test]
    fn overflow_is_clamped_to_7bit_magnitude() {
        let mut mouse = MouseController::new();
        mouse.add_mouse_delta(300, -300);

        mouse.write_strobe(false);
        let report = read_bits(&mut mouse, 32);

        assert_eq!((report >> X_MAG_START) & 0x7F, 127);
        assert_eq!((report >> Y_MAG_START) & 0x7F, 127);
        assert_eq!((report >> X_SIGN_BIT) & 1, 0);
        assert_eq!((report >> Y_SIGN_BIT) & 1, 1);
    }

    #[test]
    fn tail_bits_read_back_as_ones_after_32_bits() {
        let mut mouse = MouseController::new();
        mouse.write_strobe(false);
        let _ = read_bits(&mut mouse, 32);
        for _ in 0..8 {
            let (bit, _io) = mouse.read();
            assert!(bit);
        }
    }

    #[test]
    fn state_round_trip_preserves_mouse_fields() {
        let mut mouse = MouseController::new();
        mouse.set_mouse_left_button(true);
        mouse.set_mouse_right_button(true);
        mouse.add_mouse_delta(12, -9);
        mouse.write_strobe(true);
        mouse.write_strobe(false);

        let state = mouse.capture_state();
        let mut restored = MouseController::new();
        restored.restore_state(&state);

        let report = read_bits(&mut restored, 32);
        assert_eq!((report >> RIGHT_BUTTON_BIT) & 1, 1);
        assert_eq!((report >> LEFT_BUTTON_BIT) & 1, 1);
        assert_eq!((report >> Y_SIGN_BIT) & 1, 1);
        assert_eq!((report >> Y_MAG_START) & 0x7F, 9);
        assert_eq!((report >> X_SIGN_BIT) & 1, 0);
        assert_eq!((report >> X_MAG_START) & 0x7F, 12);
    }
}
