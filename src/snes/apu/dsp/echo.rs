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
    flg_left: u8,
    flg_right: u8,
    flg_left_sampled: bool,
    flg_right_sampled: bool,
    esa_initialized: bool,
    echo_pointer: u16,
    echo_in_l: i32,
    echo_in_r: i32,
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
            flg_left: 0,
            flg_right: 0,
            flg_left_sampled: false,
            flg_right_sampled: false,
            esa_initialized: false,
            echo_pointer: 0,
            echo_in_l: 0,
            echo_in_r: 0,
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

    pub(super) fn sample_left_echo_write_enable(&mut self, flg: u8) {
        self.flg_left = flg;
        self.flg_left_sampled = true;
    }

    pub(super) fn sample_right_echo_write_enable(&mut self, flg: u8) {
        self.flg_right = flg;
        self.flg_right_sampled = true;
    }

    /// Slot 22 (Mesen `Dsp::EchoStep22`): compute the echo pointer from the
    /// previously latched ESA and current ring offset, load the left ring
    /// word into the FIR history, and start the FIR sums with FFC0.
    pub(super) fn step_22(&mut self, aram: Option<&[u8]>, esa: u8, ffc0: i8) {
        if !self.esa_initialized {
            self.esa_latched = esa;
            self.esa_initialized = true;
        }
        self.fir_pos = (self.fir_pos + 1) & 7;
        self.echo_pointer =
            (u16::from(self.esa_latched) << 8).wrapping_add(self.ring_index.wrapping_mul(4));
        let left = read_i16_wrap_opt(aram, self.echo_pointer);
        self.fir_left[self.fir_pos] = left >> 1;
        self.echo_in_l = fir_term(self.tap_left(0), ffc0);
        self.echo_in_r = fir_term(self.tap_right(0), ffc0);
    }

    /// Slot 23 (Mesen `Dsp::EchoStep23`): load the right ring word into the
    /// FIR history and add the FFC1/FFC2 taps.
    pub(super) fn step_23(&mut self, aram: Option<&[u8]>, ffc1: i8, ffc2: i8) {
        let right = read_i16_wrap_opt(aram, self.echo_pointer.wrapping_add(2));
        self.fir_right[self.fir_pos] = right >> 1;
        self.echo_in_l += fir_term(self.tap_left(1), ffc1) + fir_term(self.tap_left(2), ffc2);
        self.echo_in_r += fir_term(self.tap_right(1), ffc1) + fir_term(self.tap_right(2), ffc2);
    }

    /// Slot 24 (Mesen `Dsp::EchoStep24`): add the FFC3/FFC4/FFC5 taps.
    pub(super) fn step_24(&mut self, ffc3: i8, ffc4: i8, ffc5: i8) {
        self.echo_in_l += fir_term(self.tap_left(3), ffc3)
            + fir_term(self.tap_left(4), ffc4)
            + fir_term(self.tap_left(5), ffc5);
        self.echo_in_r += fir_term(self.tap_right(3), ffc3)
            + fir_term(self.tap_right(4), ffc4)
            + fir_term(self.tap_right(5), ffc5);
    }

    /// Slot 25 (Mesen `Dsp::EchoStep25`): add the FFC6 tap, truncate the
    /// running sum to 16 bits, then add the individually truncated FFC7
    /// (newest sample) tap with overflow clamping and clear bit 0.
    pub(super) fn step_25(&mut self, ffc6: i8, ffc7: i8) {
        let left = i32::from((self.echo_in_l + fir_term(self.tap_left(6), ffc6)) as i16);
        let right = i32::from((self.echo_in_r + fir_term(self.tap_right(6), ffc6)) as i16);
        self.echo_in_l =
            clamp_i16_i32(left + i32::from(fir_term(self.tap_left(7), ffc7) as i16)) & !1;
        self.echo_in_r =
            clamp_i16_i32(right + i32::from(fir_term(self.tap_right(7), ffc7) as i16)) & !1;
    }

    /// The finished FIR outputs, valid from slot 25 until the next slot 22.
    #[must_use]
    pub(super) fn echo_in(&self) -> (i32, i32) {
        (self.echo_in_l, self.echo_in_r)
    }

    /// Slot 29 (Mesen `Dsp::EchoStep29`): apply EDL when the ring offset is
    /// at the buffer start, advance the offset, write the left echo sample
    /// (if enabled by the FLG value latched at slot 28), and latch ESA for
    /// the next sample's pointer.
    pub(super) fn step_29(&mut self, aram: Option<&mut [u8]>, esa: u8, edl: u8, value: i16) {
        if self.ring_index == 0 {
            self.ring_size = ring_size_from_edl(edl);
        }
        self.ring_index = self.ring_index.wrapping_add(1);
        if self.ring_index >= self.ring_size {
            self.ring_index = 0;
        }
        if let Some(aram) = aram
            && self.flg_left & 0x20 == 0
        {
            let addr = self.echo_pointer;
            if addr < 0x0010 {
                trace_apu!(4; "DSP echo writes addr=${:04X} flg=${:02X}", addr, self.flg_left);
            }
            write_i16_wrap(aram, addr, value);
        }
        self.esa_latched = esa;
        self.flg_left_sampled = false;
    }

    /// Slot 30 (Mesen `Dsp::EchoStep30`): write the right echo sample if
    /// enabled by the FLG value latched at slot 29.
    pub(super) fn step_30(&mut self, aram: Option<&mut [u8]>, value: i16) {
        if let Some(aram) = aram
            && self.flg_right & 0x20 == 0
        {
            let addr = self.echo_pointer.wrapping_add(2);
            write_i16_wrap(aram, addr, value);
        }
        self.flg_right_sampled = false;
    }

    /// Whole-sample echo pipeline for the legacy per-sample render API:
    /// runs slots 22-30 back to back with the given live register values.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn process_sample(
        &mut self,
        mut aram: Option<&mut [u8]>,
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
        if !self.flg_left_sampled {
            self.flg_left = flg;
        }
        if !self.flg_right_sampled {
            self.flg_right = flg;
        }

        self.step_22(aram.as_deref(), esa, fir_coeffs[0]);
        self.step_23(aram.as_deref(), fir_coeffs[1], fir_coeffs[2]);
        self.step_24(fir_coeffs[3], fir_coeffs[4], fir_coeffs[5]);
        self.step_25(fir_coeffs[6], fir_coeffs[7]);

        let (fir_l, fir_r) = self.echo_in();
        let mut out_l =
            clamp_i16_i32(volume_term(dry_l, master_vol_l) + volume_term(fir_l, echo_vol_l));
        let mut out_r =
            clamp_i16_i32(volume_term(dry_r, master_vol_r) + volume_term(fir_r, echo_vol_r));

        let write_l = clamp_i16_and_clear_bit0(echo_voice_l + volume_term(fir_l, echo_feedback));
        let write_r = clamp_i16_and_clear_bit0(echo_voice_r + volume_term(fir_r, echo_feedback));

        self.step_29(aram.as_deref_mut(), esa, edl, write_l);
        self.step_30(aram, write_r);

        if flg & 0x40 != 0 {
            out_l = 0;
            out_r = 0;
        }

        (out_l, out_r)
    }

    fn tap_left(&self, index: usize) -> i16 {
        self.fir_left[(self.fir_pos + index + 1) & 7]
    }

    fn tap_right(&self, index: usize) -> i16 {
        self.fir_right[(self.fir_pos + index + 1) & 7]
    }
}

fn fir_term(sample: i16, coeff: i8) -> i32 {
    (i32::from(sample) * i32::from(coeff)) >> 6
}

pub(super) fn volume_term(sample: i32, volume: i8) -> i32 {
    i32::from(((sample * i32::from(volume)) >> 7) as i16)
}

fn ring_size_from_edl(edl: u8) -> u16 {
    let units = edl & 0x0F;
    if units == 0 { 1 } else { u16::from(units) << 9 }
}

fn read_i16_wrap_opt(aram: Option<&[u8]>, addr: u16) -> i16 {
    let Some(aram) = aram else {
        return 0;
    };
    read_i16_wrap(aram, addr)
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

pub(super) fn clamp_i16_i32(value: i32) -> i32 {
    value.clamp(i32::from(i16::MIN), i32::from(i16::MAX))
}
