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
    esa_pending: u8,
    edl_pending: u8,
    flg_left: u8,
    flg_right: u8,
    flg_left_sampled: bool,
    flg_right_sampled: bool,
    esa_sampled: bool,
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
            esa_pending: 0,
            edl_pending: 0,
            flg_left: 0,
            flg_right: 0,
            flg_left_sampled: false,
            flg_right_sampled: false,
            esa_sampled: false,
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

    pub(super) fn sample_echo_registers(&mut self, esa: u8, edl: u8) {
        if !self.esa_initialized {
            self.esa_latched = esa;
            self.esa_initialized = true;
        }
        self.esa_pending = esa;
        self.edl_pending = edl;
        self.esa_sampled = true;
    }

    pub(super) fn sample_left_echo_write_enable(&mut self, flg: u8) {
        self.flg_left = flg;
        self.flg_left_sampled = true;
    }

    pub(super) fn sample_right_echo_write_enable(&mut self, flg: u8) {
        self.flg_right = flg;
        self.flg_right_sampled = true;
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
            self.esa_pending = esa;
            self.edl_pending = edl;
            self.esa_initialized = true;
        }
        if !self.esa_sampled {
            self.esa_pending = esa;
            self.edl_pending = edl;
        }
        if !self.flg_left_sampled {
            self.flg_left = flg;
        }
        if !self.flg_right_sampled {
            self.flg_right = flg;
        }

        let addr = self.ring_addr();
        let (echo_ram_l, echo_ram_r) = read_echo_entry(aram.as_deref(), addr);
        self.fir_left[self.fir_pos] = echo_ram_l >> 1;
        self.fir_right[self.fir_pos] = echo_ram_r >> 1;

        let fir_l = self.fir_sum(&self.fir_left, fir_coeffs) & !1;
        let fir_r = self.fir_sum(&self.fir_right, fir_coeffs) & !1;

        let mut out_l = volume_term(dry_l, master_vol_l) + volume_term(fir_l, echo_vol_l);
        let mut out_r = volume_term(dry_r, master_vol_r) + volume_term(fir_r, echo_vol_r);
        out_l = clamp_i16_i32(out_l);
        out_r = clamp_i16_i32(out_r);

        let write_l = clamp_i16_and_clear_bit0(echo_voice_l + volume_term(fir_l, echo_feedback));
        let write_r = clamp_i16_and_clear_bit0(echo_voice_r + volume_term(fir_r, echo_feedback));

        if let Some(aram) = aram {
            if self.flg_left & 0x20 == 0 {
                if addr < 0x0010 {
                    trace_apu!(4; "DSP echo writes addr=${:04X} flg=${:02X}", addr, flg);
                }
                if super::spc_dsp6_trace_enabled() {
                    eprintln!("neser echowrite ptr={addr:04X} L={:04X}", write_l as u16);
                }
                write_i16_wrap(aram, addr, write_l);
            }
            if self.flg_right & 0x20 == 0 {
                if super::spc_dsp6_trace_enabled() {
                    eprintln!(
                        "neser echowrite ptr={:04X} R={:04X}",
                        addr.wrapping_add(2),
                        write_r as u16
                    );
                }
                write_i16_wrap(aram, addr.wrapping_add(2), write_r);
            }
        }

        if flg & 0x40 != 0 {
            out_l = 0;
            out_r = 0;
        }

        self.advance_ring();
        self.fir_pos = (self.fir_pos + 1) & 7;
        self.esa_latched = self.esa_pending;
        self.esa_sampled = false;
        self.flg_left_sampled = false;
        self.flg_right_sampled = false;

        (out_l, out_r)
    }

    fn ring_addr(&self) -> u16 {
        (u16::from(self.esa_latched) << 8).wrapping_add(self.ring_index.wrapping_mul(4))
    }

    fn advance_ring(&mut self) {
        if self.ring_index == 0 {
            self.ring_size = ring_size_from_edl(self.edl_pending);
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
        (i32::from(sum) + i32::from(fir_term(history[newest_idx], fir_coeffs[7]) as i16))
            .clamp(i32::from(i16::MIN), i32::from(i16::MAX))
    }
}

fn fir_term(sample: i16, coeff: i8) -> i32 {
    (i32::from(sample) * i32::from(coeff)) >> 6
}

fn volume_term(sample: i32, volume: i8) -> i32 {
    i32::from(((sample * i32::from(volume)) >> 7) as i16)
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
