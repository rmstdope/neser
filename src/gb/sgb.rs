//! Minimal Super Game Boy command/input state.
//!
//! This module intentionally implements only the SGB command/input behavior
//! needed by SameSuite's `MLT_REQ` command ROMs.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SgbState {
    player_count: u8,
    current_player: u8,
    #[serde(default)]
    command: Vec<u8>,
    #[serde(default)]
    command_bits: u16,
    #[serde(default)]
    accepting_packet: bool,
    #[serde(default)]
    awaiting_stop_pulse: bool,
    #[serde(default)]
    stop_pulse_seen: bool,
}

impl Default for SgbState {
    fn default() -> Self {
        Self {
            player_count: 1,
            current_player: 0,
            command: Vec::new(),
            command_bits: 0,
            accepting_packet: false,
            awaiting_stop_pulse: false,
            stop_pulse_seen: false,
        }
    }
}

impl SgbState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn write_p1(&mut self, previous_select: u8, value: u8) {
        let select = value & 0x30;
        if previous_select & 0x20 == 0 && select & 0x20 != 0 && self.player_count & 1 == 0 {
            self.current_player = self.current_player.wrapping_add(1) & (self.player_count - 1);
        }

        match select {
            0x00 => self.reset_packet_decoder(),
            0x10 | 0x20 => self.receive_pulse(select == 0x10),
            0x30 => {
                if self.stop_pulse_seen {
                    self.execute_command();
                    self.reset_packet_decoder();
                }
            }
            _ => {}
        }
    }

    pub fn read_p1(&self, joypad_read: u8) -> u8 {
        if joypad_read & 0x30 == 0x30 {
            (joypad_read & 0xF0) | (0x0F - (self.current_player & 0x03))
        } else {
            joypad_read
        }
    }

    fn reset_packet_decoder(&mut self) {
        self.command.clear();
        self.command_bits = 0;
        self.accepting_packet = true;
        self.awaiting_stop_pulse = false;
        self.stop_pulse_seen = false;
    }

    fn receive_pulse(&mut self, one: bool) {
        if !self.accepting_packet {
            return;
        }
        if self.awaiting_stop_pulse {
            if !one {
                self.stop_pulse_seen = true;
            } else {
                self.reset_packet_decoder();
            }
            return;
        }

        if self.command_bits as usize / 8 == self.command.len() {
            self.command.push(0);
        }
        if one {
            let byte = self.command_bits as usize / 8;
            let bit = self.command_bits & 7;
            self.command[byte] |= 1 << bit;
        }
        self.command_bits += 1;

        if self.command_bits >= self.expected_command_bits() {
            self.awaiting_stop_pulse = true;
        }
    }

    fn expected_command_bits(&self) -> u16 {
        const PACKET_BITS: u16 = 16 * 8;
        if self.command_bits < 8 || self.command.is_empty() {
            return PACKET_BITS;
        }

        let first = self.command[0];
        if first & 0xF1 == 0xF1 {
            return PACKET_BITS;
        }
        let packets = first & 0x07;
        u16::from(if packets == 0 { 1 } else { packets }) * PACKET_BITS
    }

    fn execute_command(&mut self) {
        let Some(first) = self.command.first() else {
            return;
        };
        if first >> 3 != 0x11 {
            return;
        }

        let control = self.command.get(1).copied().unwrap_or(0) & 0x03;
        self.player_count = control + 1;
        if control == 2 {
            // SameSuite documents mode 2 as a glitched three-player state: it
            // performs one extra current-player step, then masks with 2.
            self.current_player = self.current_player.wrapping_add(1);
        } else if self.player_count == 3 {
            self.player_count = 4;
        }
        self.current_player &= self.player_count - 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MLT_REQ_1: [u8; 16] = [0x89, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    const MLT_REQ_3: [u8; 16] = [0x89, 0x03, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    const MLT_REQ_0: [u8; 16] = [0x89, 0x00, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    const MLT_REQ_2: [u8; 16] = [0x89, 0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

    fn write_raw(state: &mut SgbState, previous_select: &mut u8, value: u8) {
        state.write_p1(*previous_select, value);
        *previous_select = value & 0x30;
    }

    fn send_packet(state: &mut SgbState, packet: &[u8; 16]) {
        let mut select = 0x30;
        write_raw(state, &mut select, 0x00);
        write_raw(state, &mut select, 0x30);
        for byte in packet {
            let mut bits = *byte;
            for _ in 0..8 {
                write_raw(state, &mut select, if bits & 1 == 0 { 0x20 } else { 0x10 });
                write_raw(state, &mut select, 0x30);
                bits >>= 1;
            }
        }
        write_raw(state, &mut select, 0x20);
        write_raw(state, &mut select, 0x30);
    }

    fn selected_player_id(state: &SgbState) -> u8 {
        state.read_p1(0xFF) & 0x0F
    }

    fn increment(state: &mut SgbState) {
        let mut select = 0x30;
        write_raw(state, &mut select, 0x10);
        write_raw(state, &mut select, 0x30);
    }

    fn read_samesuite_result(state: &SgbState) -> u8 {
        state.read_p1(0xFF)
    }

    #[test]
    fn mlt_req_1_starts_on_player_1_then_increments_to_player_2() {
        let mut state = SgbState::new();
        send_packet(&mut state, &MLT_REQ_1);

        assert_eq!(selected_player_id(&state), 0x0F);
        let mut select = 0x30;
        write_raw(&mut state, &mut select, 0x10);
        write_raw(&mut state, &mut select, 0x30);
        assert_eq!(selected_player_id(&state), 0x0E);
    }

    #[test]
    fn mlt_req_3_cycles_four_player_ids() {
        let mut state = SgbState::new();
        send_packet(&mut state, &MLT_REQ_3);

        assert_eq!(selected_player_id(&state), 0x0F);
        let mut select = 0x30;
        for expected in [0x0E, 0x0D, 0x0C, 0x0F] {
            write_raw(&mut state, &mut select, 0x10);
            write_raw(&mut state, &mut select, 0x30);
            assert_eq!(selected_player_id(&state), expected);
        }
    }

    #[test]
    fn mlt_req_1_increment_patterns_match_samesuite() {
        let mut state = SgbState::new();
        send_packet(&mut state, &MLT_REQ_1);
        let mut select = 0x30;
        let cases: &[(&[u8], u8)] = &[
            (&[0x10, 0x30], 0x0E),
            (&[0x20, 0x30], 0x0E),
            (&[0x10, 0x20, 0x30], 0x0F),
            (&[0x10, 0x20, 0x10, 0x30], 0x0F),
            (&[0x10, 0x10, 0x30], 0x0E),
            (&[0x00, 0x10, 0x30], 0x0F),
            (&[0x10, 0x00, 0x30], 0x0E),
            (&[0x00, 0x30], 0x0F),
        ];

        for (writes, expected) in cases {
            for value in *writes {
                write_raw(&mut state, &mut select, *value);
            }
            assert_eq!(selected_player_id(&state), *expected);
        }
    }

    #[test]
    fn normal_joypad_read_passes_through_when_not_deselecting_both_groups() {
        let state = SgbState::new();

        assert_eq!(state.read_p1(0xDE), 0xDE);
    }

    #[test]
    fn mlt_req_modes_match_samesuite_command_mlt_req_results() {
        let mut state = SgbState::new();
        let mut results = Vec::new();

        send_packet(&mut state, &MLT_REQ_1);
        results.push(read_samesuite_result(&state));
        increment(&mut state);
        results.push(read_samesuite_result(&state));

        send_packet(&mut state, &MLT_REQ_0);
        send_packet(&mut state, &MLT_REQ_1);
        results.push(read_samesuite_result(&state));

        send_packet(&mut state, &MLT_REQ_0);
        send_packet(&mut state, &MLT_REQ_2);
        results.push(read_samesuite_result(&state));
        increment(&mut state);
        results.push(read_samesuite_result(&state));

        send_packet(&mut state, &MLT_REQ_0);
        send_packet(&mut state, &MLT_REQ_3);
        results.push(read_samesuite_result(&state));
        for _ in 0..3 {
            increment(&mut state);
            results.push(read_samesuite_result(&state));
        }

        for increment_count in 0..4 {
            send_packet(&mut state, &MLT_REQ_0);
            send_packet(&mut state, &MLT_REQ_3);
            for _ in 0..increment_count {
                increment(&mut state);
            }
            send_packet(&mut state, &MLT_REQ_1);
            results.push(read_samesuite_result(&state));
        }

        send_packet(&mut state, &MLT_REQ_0);
        send_packet(&mut state, &MLT_REQ_3);
        results.push(read_samesuite_result(&state));
        send_packet(&mut state, &MLT_REQ_3);
        results.push(read_samesuite_result(&state));

        for increment_count in 0..4 {
            send_packet(&mut state, &MLT_REQ_0);
            send_packet(&mut state, &MLT_REQ_3);
            for _ in 0..increment_count {
                increment(&mut state);
            }
            send_packet(&mut state, &MLT_REQ_2);
            results.push(read_samesuite_result(&state));
        }

        for increment_count in 0..3 {
            send_packet(&mut state, &MLT_REQ_0);
            send_packet(&mut state, &MLT_REQ_3);
            for _ in 0..increment_count {
                increment(&mut state);
            }
            send_packet(&mut state, &MLT_REQ_2);
            increment(&mut state);
            results.push(read_samesuite_result(&state));
            if increment_count < 2 {
                increment(&mut state);
                results.push(read_samesuite_result(&state));
            }
        }

        assert_eq!(
            results,
            [
                0xFF, 0xFE, 0xFF, 0xFF, 0xFF, 0xFF, 0xFE, 0xFD, 0xFC, 0xFE, 0xFF, 0xFE, 0xFF, 0xFF,
                0xFD, 0xFD, 0xFD, 0xFF, 0xFF, 0xFD, 0xFD, 0xFD, 0xFD, 0xFF,
            ]
        );
    }
}
