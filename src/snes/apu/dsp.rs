//! SNES S-DSP voice pipeline (work in progress).

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedBrrBlock {
    pub samples: [i16; 16],
    pub loop_flag: bool,
    pub end_flag: bool,
}

use serde::{Deserialize, Serialize};
const DSP_REGISTER_COUNT: usize = 0x80;
const GAUSSIAN_LUT: [i16; 512] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2,
    2, 2, 3, 3, 3, 3, 3, 4, 4, 4, 4, 4, 5, 5, 5, 5, 6, 6, 6, 6, 7, 7, 7, 8, 8, 8, 9, 9, 9, 10, 10,
    10, 11, 11, 11, 12, 12, 13, 13, 14, 14, 15, 15, 15, 16, 16, 17, 17, 18, 19, 19, 20, 20, 21, 21,
    22, 23, 23, 24, 24, 25, 26, 27, 27, 28, 29, 29, 30, 31, 32, 32, 33, 34, 35, 36, 36, 37, 38, 39,
    40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 58, 59, 60, 61, 62, 64, 65,
    66, 67, 69, 70, 71, 73, 74, 76, 77, 78, 80, 81, 83, 84, 86, 87, 89, 90, 92, 94, 95, 97, 99,
    100, 102, 104, 106, 107, 109, 111, 113, 115, 117, 118, 120, 122, 124, 126, 128, 130, 132, 134,
    137, 139, 141, 143, 145, 147, 150, 152, 154, 156, 159, 161, 163, 166, 168, 171, 173, 175, 178,
    180, 183, 186, 188, 191, 193, 196, 199, 201, 204, 207, 210, 212, 215, 218, 221, 224, 227, 230,
    233, 236, 239, 242, 245, 248, 251, 254, 257, 260, 263, 267, 270, 273, 276, 280, 283, 286, 290,
    293, 297, 300, 304, 307, 311, 314, 318, 321, 325, 328, 332, 336, 339, 343, 347, 351, 354, 358,
    362, 366, 370, 374, 378, 381, 385, 389, 393, 397, 401, 405, 410, 414, 418, 422, 426, 430, 434,
    439, 443, 447, 451, 456, 460, 464, 469, 473, 477, 482, 486, 491, 495, 499, 504, 508, 513, 517,
    522, 527, 531, 536, 540, 545, 550, 554, 559, 563, 568, 573, 577, 582, 587, 592, 596, 601, 606,
    611, 615, 620, 625, 630, 635, 640, 644, 649, 654, 659, 664, 669, 674, 678, 683, 688, 693, 698,
    703, 708, 713, 718, 723, 728, 732, 737, 742, 747, 752, 757, 762, 767, 772, 777, 782, 787, 792,
    797, 802, 806, 811, 816, 821, 826, 831, 836, 841, 846, 851, 855, 860, 865, 870, 875, 880, 884,
    889, 894, 899, 904, 908, 913, 918, 923, 927, 932, 937, 941, 946, 951, 955, 960, 965, 969, 974,
    978, 983, 988, 992, 997, 1001, 1005, 1010, 1014, 1019, 1023, 1027, 1032, 1036, 1040, 1045,
    1049, 1053, 1057, 1061, 1066, 1070, 1074, 1078, 1082, 1086, 1090, 1094, 1098, 1102, 1106, 1109,
    1113, 1117, 1121, 1125, 1128, 1132, 1136, 1139, 1143, 1146, 1150, 1153, 1157, 1160, 1164, 1167,
    1170, 1174, 1177, 1180, 1183, 1186, 1190, 1193, 1196, 1199, 1202, 1205, 1207, 1210, 1213, 1216,
    1219, 1221, 1224, 1227, 1229, 1232, 1234, 1237, 1239, 1241, 1244, 1246, 1248, 1251, 1253, 1255,
    1257, 1259, 1261, 1263, 1265, 1267, 1269, 1270, 1272, 1274, 1275, 1277, 1279, 1280, 1282, 1283,
    1284, 1286, 1287, 1288, 1290, 1291, 1292, 1293, 1294, 1295, 1296, 1297, 1297, 1298, 1299, 1300,
    1300, 1301, 1302, 1302, 1303, 1303, 1303, 1304, 1304, 1304, 1304, 1304, 1305, 1305,
];

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
    #[serde(default)]
    voice_vol_l: [i8; 8],
    #[serde(default)]
    voice_vol_r: [i8; 8],
    #[serde(default)]
    master_vol_l: i8,
    #[serde(default)]
    master_vol_r: i8,
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
            voice_vol_l: [0; 8],
            voice_vol_r: [0; 8],
            master_vol_l: 0,
            master_vol_r: 0,
        }
    }

    pub fn normalize_after_restore(&mut self) -> Result<(), String> {
        self.phase &= 0x1F;
        if self.regs.is_empty() {
            self.regs = default_regs();
        }
        if self.regs.len() != DSP_REGISTER_COUNT {
            return Err(format!(
                "APU DSP register file size mismatch (expected {DSP_REGISTER_COUNT}, found {})",
                self.regs.len()
            ));
        }
        self.rebuild_cached_fields_from_regs();
        Ok(())
    }

    fn rebuild_cached_fields_from_regs(&mut self) {
        self.master_vol_l = self.regs[0x0C] as i8;
        self.master_vol_r = self.regs[0x1C] as i8;

        for voice in 0..8usize {
            let base = voice << 4;
            self.voice_vol_l[voice] = self.regs[base] as i8;
            self.voice_vol_r[voice] = self.regs[base + 1] as i8;
            self.voice_pitch[voice] =
                u16::from(self.regs[base + 2]) | (u16::from(self.regs[base + 3] & 0x3F) << 8);
        }
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

    pub fn set_voice_volume(&mut self, voice: usize, left: i8, right: i8) {
        let idx = voice_index(voice);
        self.voice_vol_l[idx] = left;
        self.voice_vol_r[idx] = right;
    }

    pub fn set_master_volume(&mut self, left: i8, right: i8) {
        self.master_vol_l = left;
        self.master_vol_r = right;
    }

    #[must_use]
    pub fn mix_voice_sample(&self, voice: usize, sample: i16) -> (i16, i16) {
        let idx = voice_index(voice);
        let left = apply_two_stage_volume(sample, self.voice_vol_l[idx], self.master_vol_l);
        let right = apply_two_stage_volume(sample, self.voice_vol_r[idx], self.master_vol_r);
        (left, right)
    }

    #[must_use]
    pub fn gaussian_interpolate(&self, s0: i16, s1: i16, s2: i16, s3: i16, frac: u8) -> i16 {
        let offset = usize::from(frac);
        let fwd = 255 - offset;
        let rev = offset;

        let mut out = (i32::from(GAUSSIAN_LUT[fwd]) * i32::from(s0)) >> 11;
        out += (i32::from(GAUSSIAN_LUT[fwd + 256]) * i32::from(s1)) >> 11;
        out += (i32::from(GAUSSIAN_LUT[rev + 256]) * i32::from(s2)) >> 11;
        out = i32::from(out as i16);
        out += (i32::from(GAUSSIAN_LUT[rev]) * i32::from(s3)) >> 11;
        out = out.clamp(i32::from(i16::MIN), i32::from(i16::MAX));
        (out as i16) & !1
    }

    pub fn step_phase(&mut self) {
        self.phase = self.phase.wrapping_add(1) & 0x1F;
    }

    pub fn write_reg(&mut self, addr: u8, value: u8) {
        let reg = addr & 0x7F;
        let index = usize::from(reg);
        debug_assert_eq!(self.regs.len(), DSP_REGISTER_COUNT);
        self.regs[index] = value;

        if reg == 0x0C {
            self.master_vol_l = value as i8;
            return;
        }
        if reg == 0x1C {
            self.master_vol_r = value as i8;
            return;
        }

        let voice = usize::from(reg >> 4);
        if voice >= 8 {
            return;
        }
        match reg & 0x0F {
            0x00 => self.voice_vol_l[voice] = value as i8,
            0x01 => self.voice_vol_r[voice] = value as i8,
            0x02 => {
                let prev = self.voice_pitch[voice];
                self.voice_pitch[voice] = (prev & 0x3F00) | u16::from(value);
            }
            0x03 => {
                let prev = self.voice_pitch[voice];
                self.voice_pitch[voice] = (prev & 0x00FF) | (u16::from(value & 0x3F) << 8);
            }
            _ => {}
        }
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

fn apply_two_stage_volume(sample: i16, voice_vol: i8, master_vol: i8) -> i16 {
    let mut scaled = i32::from(sample) * i32::from(voice_vol);
    scaled >>= 7;
    scaled = (scaled * i32::from(master_vol)) >> 7;
    scaled.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
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

    #[test]
    fn given_constant_samples_when_gaussian_interpolating_then_output_preserves_level() {
        let dsp = Sdsp::new();
        let out = dsp.gaussian_interpolate(100, 100, 100, 100, 64);
        assert!((out - 100).abs() <= 2);
        assert_eq!(out & 1, 0);
    }

    #[test]
    fn given_fraction_endpoints_when_gaussian_interpolating_then_indices_are_safe() {
        let dsp = Sdsp::new();
        let out0 = dsp.gaussian_interpolate(-123, 456, -789, 321, 0);
        let out255 = dsp.gaussian_interpolate(-123, 456, -789, 321, 255);
        assert_eq!(out0 & 1, 0);
        assert_eq!(out255 & 1, 0);
    }

    #[test]
    fn given_extreme_samples_when_gaussian_interpolating_then_output_is_clamped_and_even() {
        let dsp = Sdsp::new();
        let out = dsp.gaussian_interpolate(i16::MIN, i16::MAX, i16::MIN, i16::MAX, 127);
        assert_eq!(out & 1, 0);
    }

    #[test]
    fn given_voice_and_master_volume_when_mixing_then_both_are_applied() {
        let mut dsp = Sdsp::new();
        dsp.set_voice_volume(0, 64, 32);
        dsp.set_master_volume(127, 127);

        let (left, right) = dsp.mix_voice_sample(0, 1000);
        assert_eq!(left, 496);
        assert_eq!(right, 248);
    }

    #[test]
    fn given_volume_register_writes_when_mixing_then_register_driven_values_are_used() {
        let mut dsp = Sdsp::new();
        dsp.write_reg(0x00, 64);
        dsp.write_reg(0x01, 32);
        dsp.write_reg(0x0C, 127);
        dsp.write_reg(0x1C, 127);

        let (left, right) = dsp.mix_voice_sample(0, 1000);
        assert_eq!(left, 496);
        assert_eq!(right, 248);
    }

    #[test]
    fn given_pitch_register_writes_when_stepping_then_accumulator_uses_vxpitch() {
        let mut dsp = Sdsp::new();
        dsp.write_reg(0x22, 0x34);
        dsp.write_reg(0x23, 0x12);

        dsp.step_voice_pitch(2);
        assert_eq!(dsp.voice_sample_pos(2), 0x1234);
    }

    #[test]
    fn normalize_after_restore_rebuilds_cached_pitch_and_volume_fields() {
        let mut dsp = Sdsp::new();
        dsp.write_reg(0x00, 64);
        dsp.write_reg(0x01, 32);
        dsp.write_reg(0x0C, 127);
        dsp.write_reg(0x1C, 127);
        dsp.write_reg(0x02, 0x34);
        dsp.write_reg(0x03, 0x12);

        dsp.voice_vol_l[0] = 0;
        dsp.voice_vol_r[0] = 0;
        dsp.master_vol_l = 0;
        dsp.master_vol_r = 0;
        dsp.voice_pitch[0] = 0;

        dsp.normalize_after_restore()
            .expect("normalize should rebuild cached fields");
        let (left, right) = dsp.mix_voice_sample(0, 1000);
        assert_eq!(left, 496);
        assert_eq!(right, 248);

        dsp.step_voice_pitch(0);
        assert_eq!(dsp.voice_sample_pos(0), 0x1234);
    }

    #[test]
    fn normalize_after_restore_masks_phase_to_5_bits() {
        let mut dsp = Sdsp::new();
        dsp.phase = 0xFF;
        dsp.normalize_after_restore()
            .expect("normalize should accept default register file");
        assert_eq!(dsp.phase(), 0x1F);
    }
}
