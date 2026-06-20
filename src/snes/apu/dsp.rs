//! SNES S-DSP voice pipeline (work in progress).

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedBrrBlock {
    pub samples: [i16; 16],
    pub loop_flag: bool,
    pub end_flag: bool,
}

use serde::{Deserialize, Serialize};
const DSP_REGISTER_COUNT: usize = 0x80;

fn default_regs() -> Vec<u8> {
    vec![0; DSP_REGISTER_COUNT]
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Sdsp {
    #[serde(default)]
    phase: u8,
    #[serde(default = "default_regs")]
    regs: Vec<u8>,
    #[serde(default)]
    voice_pitch: [u16; 8],
    #[serde(default)]
    voice_sample_pos: [u32; 8],
}

impl Default for Sdsp {
    fn default() -> Self {
        Self::new()
    }
}

impl Sdsp {
    #[must_use]
    pub fn new() -> Self {
        Self {
            phase: 0,
            regs: default_regs(),
            voice_pitch: [0; 8],
            voice_sample_pos: [0; 8],
        }
    }

    pub fn normalize_after_restore(&mut self) -> Result<(), String> {
        if self.regs.is_empty() {
            self.regs = default_regs();
            return Ok(());
        }
        if self.regs.len() != DSP_REGISTER_COUNT {
            return Err(format!(
                "APU DSP register file size mismatch (expected {DSP_REGISTER_COUNT}, found {})",
                self.regs.len()
            ));
        }
        Ok(())
    }

    #[must_use]
    pub fn phase(&self) -> u8 {
        self.phase
    }

    pub fn set_voice_pitch(&mut self, voice: usize, pitch: u16) {
        let idx = voice_index(voice);
        self.voice_pitch[idx] = pitch & 0x3FFF;
    }

    #[must_use]
    pub fn voice_sample_pos(&self, voice: usize) -> u32 {
        self.voice_sample_pos[voice_index(voice)]
    }

    pub fn step_voice_pitch(&mut self, voice: usize) {
        let idx = voice_index(voice);
        self.voice_sample_pos[idx] =
            self.voice_sample_pos[idx].wrapping_add(u32::from(self.voice_pitch[idx]));
    }

    pub fn step_phase(&mut self) {
        self.phase = self.phase.wrapping_add(1) & 0x1F;
    }

    pub fn write_reg(&mut self, addr: u8, value: u8) {
        let index = usize::from(addr & 0x7F);
        debug_assert_eq!(self.regs.len(), DSP_REGISTER_COUNT);
        self.regs[index] = value;
    }

    #[must_use]
    pub fn read_reg(&self, addr: u8) -> u8 {
        let index = usize::from(addr & 0x7F);
        self.regs.get(index).copied().unwrap_or(0)
    }

    #[must_use]
    pub fn decode_brr_block(header: u8, data: [u8; 8], prev1: i16, prev2: i16) -> DecodedBrrBlock {
        let shift = (header >> 4) & 0x0F;
        let filter = (header >> 2) & 0x03;
        let mut hist1 = prev1;
        let mut hist2 = prev2;
        let mut samples = [0i16; 16];
        for (i, byte) in data.iter().copied().enumerate() {
            let hi = ((byte >> 4) & 0x0F) as i8;
            let lo = (byte & 0x0F) as i8;
            let s0 = decode_brr_nibble(sign_extend_nibble(hi), shift, filter, hist1, hist2);
            hist2 = hist1;
            hist1 = s0;
            samples[i * 2] = s0;

            let s1 = decode_brr_nibble(sign_extend_nibble(lo), shift, filter, hist1, hist2);
            hist2 = hist1;
            hist1 = s1;
            samples[i * 2 + 1] = s1;
        }
        DecodedBrrBlock {
            samples,
            loop_flag: header & 0x02 != 0,
            end_flag: header & 0x01 != 0,
        }
    }
}

fn sign_extend_nibble(value: i8) -> i16 {
    let widened = (value << 4) >> 4;
    i16::from(widened)
}

fn decode_brr_nibble(raw: i16, shift: u8, filter: u8, prev1: i16, prev2: i16) -> i16 {
    let base = if shift > 12 {
        if raw >= 0 { 0 } else { -2048 }
    } else {
        i32::from(raw) << shift
    };
    let predict = match filter {
        0 => 0,
        1 => (i32::from(prev1) * 15) >> 4,
        2 => ((i32::from(prev1) * 61) >> 5) - ((i32::from(prev2) * 15) >> 4),
        _ => ((i32::from(prev1) * 115) >> 6) - ((i32::from(prev2) * 13) >> 4),
    };
    (base + predict).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

fn voice_index(voice: usize) -> usize {
    assert!(voice < 8, "voice index out of range: {voice}");
    voice
}

#[cfg(test)]
mod tests {
    use super::Sdsp;

    #[test]
    fn given_phase_31_when_step_phase_then_wraps_to_0() {
        let mut dsp = Sdsp::new();
        for _ in 0..31 {
            dsp.step_phase();
        }
        assert_eq!(dsp.phase(), 31);

        dsp.step_phase();
        assert_eq!(dsp.phase(), 0);
    }

    #[test]
    fn given_all_register_addresses_when_written_then_reads_back_same_value() {
        let mut dsp = Sdsp::new();
        for addr in 0u8..=0x7F {
            let value = addr.wrapping_mul(3).wrapping_add(1);
            dsp.write_reg(addr, value);
        }

        for addr in 0u8..=0x7F {
            let value = addr.wrapping_mul(3).wrapping_add(1);
            assert_eq!(dsp.read_reg(addr), value, "addr=0x{addr:02X}");
        }
    }

    #[test]
    fn given_mirrored_register_addresses_when_written_then_base_registers_match() {
        let mut dsp = Sdsp::new();

        dsp.write_reg(0x95, 0xAB);
        assert_eq!(dsp.read_reg(0x15), 0xAB);
        assert_eq!(dsp.read_reg(0x95), 0xAB);
    }

    #[test]
    fn given_brr_header_with_loop_and_end_bits_when_decoded_then_flags_are_exposed() {
        let header = 0b0000_0011;
        let decoded = Sdsp::decode_brr_block(header, [0; 8], 0, 0);
        assert!(decoded.loop_flag);
        assert!(decoded.end_flag);
    }

    #[test]
    fn given_filter0_shift0_nibbles_when_decoded_then_positive_and_negative_samples_survive() {
        let header = 0b0000_0000;
        let mut data = [0u8; 8];
        data[0] = 0x78;
        data[1] = 0x0F;

        let decoded = Sdsp::decode_brr_block(header, data, 0, 0);

        assert_eq!(decoded.samples[0], 7);
        assert_eq!(decoded.samples[1], -8);
        assert_eq!(decoded.samples[2], 0);
        assert_eq!(decoded.samples[3], -1);
    }

    #[test]
    fn given_higher_range_when_decoded_then_sample_magnitude_increases() {
        let mut data = [0u8; 8];
        data[0] = 0x10;

        let shift0 = Sdsp::decode_brr_block(0b0000_0000, data, 0, 0);
        let shift1 = Sdsp::decode_brr_block(0b0001_0000, data, 0, 0);

        assert!(shift1.samples[0].abs() > shift0.samples[0].abs());
    }

    #[test]
    fn given_filter1_and_prev_sample_when_decoded_then_history_influences_output() {
        let mut data = [0u8; 8];
        data[0] = 0x00;

        let decoded = Sdsp::decode_brr_block(0b0000_0100, data, 16, 0);
        assert!(decoded.samples[0] > 0);
    }

    #[test]
    fn given_voice_pitch_when_step_voice_pitch_then_sample_position_advances() {
        let mut dsp = Sdsp::new();
        dsp.set_voice_pitch(2, 0x1234);

        dsp.step_voice_pitch(2);
        assert_eq!(dsp.voice_sample_pos(2), 0x1234);
    }
}
