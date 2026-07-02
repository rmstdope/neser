use crate::trace_apu;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub(super) struct EchoState {
    ring_index: u16,
    ring_size: u16,
    fir_pos: usize,
    fir_left: [i16; 8],
    fir_right: [i16; 8],
    esa_latched: u8,
    esa_initialized: bool,
}

impl Default for EchoState {
    fn default() -> Self {
        Self::new()
    }
}

impl EchoState {
    #[must_use]
    pub(super) fn new() -> Self {
        Self {
            ring_index: 0,
            ring_size: 1,
            fir_pos: 0,
            fir_left: [0; 8],
            fir_right: [0; 8],
            esa_latched: 0,
            esa_initialized: false,
        }
    }

    pub(super) fn normalize_after_restore(&mut self) {
        self.fir_pos &= 7;
        if self.ring_size == 0 {
            self.ring_size = 1;
        }
        if self.ring_index >= self.ring_size {
            self.ring_index = 0;
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn process_sample(
        &mut self,
        aram: Option<&mut [u8]>,
        esa: u8,
        edl: u8,
        fir_coeffs: &[i8; 8],
        echo_feedback: i8,
        echo_vol_l: i8,
        echo_vol_r: i8,
        master_vol_l: i8,
        master_vol_r: i8,
        flg: u8,
        echo_voice_l: i32,
        echo_voice_r: i32,
        dry_l: i32,
        dry_r: i32,
    ) -> (i32, i32) {
        if !self.esa_initialized {
            self.esa_latched = esa;
            self.esa_initialized = true;
        }

        let addr = self.ring_addr();
        let (echo_ram_l, echo_ram_r) = read_echo_entry(aram.as_deref(), addr);
        self.fir_left[self.fir_pos] = echo_ram_l >> 1;
        self.fir_right[self.fir_pos] = echo_ram_r >> 1;

        let fir_l = self.fir_sum(&self.fir_left, fir_coeffs);
        let fir_r = self.fir_sum(&self.fir_right, fir_coeffs);

        let mut out_l =
            ((dry_l * i32::from(master_vol_l)) >> 7) + ((fir_l * i32::from(echo_vol_l)) >> 7);
        let mut out_r =
            ((dry_r * i32::from(master_vol_r)) >> 7) + ((fir_r * i32::from(echo_vol_r)) >> 7);
        out_l = clamp_i16_i32(out_l);
        out_r = clamp_i16_i32(out_r);

        let write_l =
            clamp_i16_and_clear_bit0(echo_voice_l + ((fir_l * i32::from(echo_feedback)) >> 7));
        let write_r =
            clamp_i16_and_clear_bit0(echo_voice_r + ((fir_r * i32::from(echo_feedback)) >> 7));

        if flg & 0x20 == 0
            && let Some(aram) = aram
        {
            if addr < 0x0010 {
                trace_apu!(4; "DSP echo writes addr=${:04X} flg=${:02X}", addr, flg);
            }
            write_echo_entry(aram, addr, write_l, write_r);
        }

        if flg & 0x40 != 0 {
            out_l = 0;
            out_r = 0;
        }

        self.advance_ring(edl);
        self.fir_pos = (self.fir_pos + 1) & 7;
        self.esa_latched = esa;

        (out_l, out_r)
    }

    fn ring_addr(&self) -> u16 {
        (u16::from(self.esa_latched) << 8).wrapping_add(self.ring_index.wrapping_mul(4))
    }

    fn advance_ring(&mut self, edl: u8) {
        if self.ring_index == 0 {
            self.ring_size = ring_size_from_edl(edl);
        }
        self.ring_index = self.ring_index.wrapping_add(1);
        if self.ring_index >= self.ring_size {
            self.ring_index = 0;
        }
    }

    fn fir_sum(&self, history: &[i16; 8], fir_coeffs: &[i8; 8]) -> i32 {
        let mut sum = 0i16;
        for (tap, coeff) in fir_coeffs.iter().take(7).enumerate() {
            let hist_idx = (self.fir_pos + tap + 1) & 7;
            sum = sum.wrapping_add(fir_term(history[hist_idx], *coeff) as i16);
        }
        let newest_idx = self.fir_pos;
        (i32::from(sum) + fir_term(history[newest_idx], fir_coeffs[7]))
            .clamp(i32::from(i16::MIN), i32::from(i16::MAX))
    }
}

fn fir_term(sample: i16, coeff: i8) -> i32 {
    (i32::from(sample) * i32::from(coeff)) >> 6
}

fn ring_size_from_edl(edl: u8) -> u16 {
    let units = edl & 0x0F;
    if units == 0 { 1 } else { u16::from(units) << 9 }
}

fn read_echo_entry(aram: Option<&[u8]>, addr: u16) -> (i16, i16) {
    let Some(aram) = aram else {
        return (0, 0);
    };
    let left = read_i16_wrap(aram, addr);
    let right = read_i16_wrap(aram, addr.wrapping_add(2));
    (left, right)
}

fn write_echo_entry(aram: &mut [u8], addr: u16, left: i16, right: i16) {
    write_i16_wrap(aram, addr, left);
    write_i16_wrap(aram, addr.wrapping_add(2), right);
}

fn read_i16_wrap(aram: &[u8], addr: u16) -> i16 {
    if aram.is_empty() {
        return 0;
    }
    let lo_idx = usize::from(addr) % aram.len();
    let hi_idx = (lo_idx + 1) % aram.len();
    i16::from_le_bytes([aram[lo_idx], aram[hi_idx]])
}

fn write_i16_wrap(aram: &mut [u8], addr: u16, value: i16) {
    if aram.is_empty() {
        return;
    }
    let bytes = value.to_le_bytes();
    let lo_idx = usize::from(addr) % aram.len();
    let hi_idx = (lo_idx + 1) % aram.len();
    aram[lo_idx] = bytes[0];
    aram[hi_idx] = bytes[1];
}

fn clamp_i16_and_clear_bit0(value: i32) -> i16 {
    let clamped = clamp_i16_i32(value) as i16;
    clamped & !1
}

fn clamp_i16_i32(value: i32) -> i32 {
    value.clamp(i32::from(i16::MIN), i32::from(i16::MAX))
}
