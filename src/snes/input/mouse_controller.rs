//! SNES Mouse controller.
//!
//! Serial output format follows the SNES mouse protocol:
//! - bits 1..=8: unused, always 0
//! - bit 9: right button
//! - bit 10: left button
//! - bits 11..=12: sensitivity
//! - bits 13..=16: hardware ID (0001b)
//! - bit 17: vertical direction (1=up, 0=down)
//! - bits 18..=24: vertical 7-bit magnitude
//! - bit 25: horizontal direction (1=left, 0=right)
//! - bits 26..=32: horizontal 7-bit magnitude
//!
//! Each byte is shifted MSB-first, matching the rest of the SNES controller stack.

use super::{SnesController, SnesControllerState};

const REPORT_BITS: u8 = 32;
const MAGNITUDE_MAX: i16 = 127;

const SPEED_NORMAL: u8 = 0;
const SPEED_NORMAL_ACCEL: u8 = 1;
const SPEED_FAST: u8 = 2;
const SPEED_WRAP: u8 = 3;

#[derive(Debug, Clone)]
pub struct MouseController {
    speed: u8,
    left_button: bool,
    right_button: bool,
    accum_dx: i16,
    accum_dy: i16,
    report_dx: i16,
    report_dy: i16,
    packet: [u8; 4],
    shift_index: u8,
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
            packet: [0; 4],
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

    fn sensitivity_multiplier(speed: u8) -> i16 {
        match speed & 0x03 {
            SPEED_NORMAL => 1,
            SPEED_NORMAL_ACCEL => 2,
            SPEED_FAST => 3,
            _ => 1,
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
        self.packet = self.build_packet();
        self.shift_index = 0;
    }

    fn cycle_speed(&mut self) {
        self.speed = match self.speed {
            SPEED_NORMAL => SPEED_NORMAL_ACCEL,
            SPEED_NORMAL_ACCEL => SPEED_FAST,
            SPEED_FAST => SPEED_NORMAL,
            _ => SPEED_NORMAL,
        };
    }

    fn build_packet(&self) -> [u8; 4] {
        let (vertical_negative, vertical_magnitude) = Self::sign_and_magnitude(self.report_dy);
        let (horizontal_negative, horizontal_magnitude) = Self::sign_and_magnitude(self.report_dx);

        let right_button = if self.right_button { 0x80 } else { 0x00 };
        let left_button = if self.left_button { 0x40 } else { 0x00 };
        let sensitivity = (self.speed & 0x03) << 4;
        let mouse_id = 0x01;

        let vertical_direction = if vertical_negative { 0x80 } else { 0x00 };
        let horizontal_direction = if horizontal_negative { 0x80 } else { 0x00 };

        [
            0x00,
            right_button | left_button | sensitivity | mouse_id,
            vertical_direction | (vertical_magnitude & 0x7F),
            horizontal_direction | (horizontal_magnitude & 0x7F),
        ]
    }

    fn current_bit(&self) -> bool {
        if self.shift_index >= REPORT_BITS {
            return true;
        }

        let byte = self.packet[(self.shift_index / 8) as usize];
        let bit_in_byte = self.shift_index % 8;
        ((byte >> (7 - bit_in_byte)) & 1) != 0
    }
}

impl SnesController for MouseController {
    fn write_strobe(&mut self, high: bool) {
        self.strobe = high;

        if high {
            self.shift_index = 0;
        } else {
            self.latch_report();
        }
    }

    fn read(&mut self) -> (bool, bool) {
        let bit = if self.strobe {
            self.cycle_speed();
            self.current_bit()
        } else {
            let current = self.current_bit();
            self.shift_index = self.shift_index.saturating_add(1);
            current
        };

        (bit, false)
    }

    fn set_button(&mut self, _button: super::SnesButton, _pressed: bool) -> bool {
        false
    }

    fn add_mouse_delta(&mut self, dx: i16, dy: i16) -> bool {
        let multiplier = Self::sensitivity_multiplier(self.speed);
        Self::add_clamped_with_remainder(&mut self.accum_dx, dx.saturating_mul(multiplier));
        Self::add_clamped_with_remainder(&mut self.accum_dy, dy.saturating_mul(multiplier));
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
            shift: self.shift_index,
            strobe: self.strobe,
            mouse_speed: self.speed,
            mouse_left_button: self.left_button,
            mouse_right_button: self.right_button,
            mouse_accum_dx: self.accum_dx,
            mouse_accum_dy: self.accum_dy,
            mouse_report_dx: self.report_dx,
            mouse_report_dy: self.report_dy,
            superscope_x: 0,
            superscope_y: 0,
            superscope_trigger: false,
            superscope_cursor: false,
            superscope_turbo: false,
            superscope_pause: false,
            superscope_offscreen: false,
            superscope_turbo_enabled: false,
            superscope_turbo_lock: false,
            superscope_trigger_output: false,
            superscope_pause_output: false,
            superscope_trigger_lock: false,
            superscope_pause_lock: false,
            superscope_latched: false,
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
        self.packet = self.build_packet();
        self.shift_index = state.shift.min(REPORT_BITS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn latch(mouse: &mut MouseController) {
        mouse.write_strobe(true);
        mouse.write_strobe(false);
    }

    fn read_bits(mouse: &mut MouseController, count: usize) -> Vec<bool> {
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            let (bit, _io) = mouse.read();
            out.push(bit);
        }
        out
    }

    fn bits_to_byte(bits: &[bool]) -> u8 {
        bits.iter()
            .fold(0u8, |acc, bit| (acc << 1) | u8::from(*bit))
    }

    fn read_packet(mouse: &mut MouseController) -> [u8; 4] {
        let bits = read_bits(mouse, 32);
        [
            bits_to_byte(&bits[0..8]),
            bits_to_byte(&bits[8..16]),
            bits_to_byte(&bits[16..24]),
            bits_to_byte(&bits[24..32]),
        ]
    }

    #[test]
    fn default_speed_is_normal_after_first_latch() {
        let mut mouse = MouseController::new();
        latch(&mut mouse);
        let packet = read_packet(&mut mouse);
        assert_eq!(packet[1] & 0x30, 0x00);
    }

    #[test]
    fn clock_while_strobe_high_cycles_speed_mode() {
        let mut mouse = MouseController::new();

        mouse.write_strobe(true);
        let _ = mouse.read();
        mouse.write_strobe(false);
        let slow = read_packet(&mut mouse);
        assert_eq!(slow[1] & 0x30, 0x10);

        mouse.write_strobe(true);
        let _ = mouse.read();
        mouse.write_strobe(false);
        let fast = read_packet(&mut mouse);
        assert_eq!(fast[1] & 0x30, 0x20);

        mouse.write_strobe(true);
        let _ = mouse.read();
        mouse.write_strobe(false);
        let normal = read_packet(&mut mouse);
        assert_eq!(normal[1] & 0x30, 0x00);
    }

    #[test]
    fn report_contains_buttons_and_signed_magnitudes() {
        let mut mouse = MouseController::new();
        mouse.set_mouse_left_button(true);
        mouse.set_mouse_right_button(true);
        mouse.add_mouse_delta(-5, 6);

        latch(&mut mouse);
        let packet = read_packet(&mut mouse);

        assert_eq!(packet[1], 0xC1);
        assert_eq!(packet[2], 0x06);
        assert_eq!(packet[3], 0x85);
    }

    #[test]
    fn overflow_is_clamped_to_7bit_magnitude() {
        let mut mouse = MouseController::new();
        mouse.add_mouse_delta(300, -300);

        latch(&mut mouse);
        let packet = read_packet(&mut mouse);

        assert_eq!(packet[2], 0xFF);
        assert_eq!(packet[3], 0x7F);
    }

    #[test]
    fn tail_bits_read_back_as_ones_after_32_bits() {
        let mut mouse = MouseController::new();
        latch(&mut mouse);
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
        latch(&mut mouse);
        let _ = mouse.read();
        let _ = mouse.read();

        let state = mouse.capture_state();
        let expected_remaining_bits = read_bits(&mut mouse, 30);
        let mut restored = MouseController::new();
        restored.restore_state(&state);

        let restored_remaining_bits = read_bits(&mut restored, 30);
        assert_eq!(restored_remaining_bits, expected_remaining_bits);
    }

    #[test]
    fn sensitivity_scales_motion_before_latch() {
        let mut mouse = MouseController::new();
        mouse.write_strobe(true);
        let _ = mouse.read();
        mouse.write_strobe(false);
        mouse.add_mouse_delta(4, 3);

        latch(&mut mouse);
        let packet = read_packet(&mut mouse);

        assert_eq!(packet[1] & 0x30, 0x10);
        assert_eq!(packet[2] & 0x7F, 6);
        assert_eq!(packet[3] & 0x7F, 8);
    }
}
