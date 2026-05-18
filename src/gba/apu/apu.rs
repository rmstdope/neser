//! GBA APU — dual-source audio: 4 DMG-legacy channels + 2 PCM FIFO channels.

pub use super::channel1::Channel1;
pub use super::channel2::Channel2;
pub use super::channel3::Channel3;
pub use super::channel4::Channel4;
pub use super::fifo::FifoChannel;

// ── Timing constants ──────────────────────────────────────────────────────────

/// GBA CPU clock rate in Hz.
const GBA_CLOCK_HZ: f32 = 16_777_216.0;

/// Frame sequencer period: 512 Hz → one step every 32 768 GBA cycles.
const FS_PERIOD: u32 = 32_768;

// ── Duty cycle waveforms ──────────────────────────────────────────────────────

/// 8-step duty-cycle waveforms (identical to DMG APU).
pub(super) const DUTY_TABLE: [[u8; 8]; 4] = [
    [0, 0, 0, 0, 0, 0, 0, 1], // 12.5 %
    [1, 0, 0, 0, 0, 0, 0, 1], // 25 %
    [1, 0, 0, 0, 0, 1, 1, 1], // 50 %
    [0, 1, 1, 1, 1, 1, 1, 0], // 75 %
];

// ── Frame Sequencer step table ────────────────────────────────────────────────

/// Frame Sequencer step flags (same as DMG APU):
/// bit 0 = clock length, bit 1 = clock sweep, bit 2 = clock envelope.
#[rustfmt::skip]
const FS_TABLE: [u8; 8] = [
    0b001, // step 0: length
    0b000, // step 1: —
    0b011, // step 2: length + sweep
    0b000, // step 3: —
    0b001, // step 4: length
    0b000, // step 5: —
    0b011, // step 6: length + sweep
    0b100, // step 7: envelope
];

// ── GBA APU ───────────────────────────────────────────────────────────────────

/// GBA APU: 4 DMG-legacy channels + 2 PCM FIFO channels.
#[derive(Debug, Clone)]
pub struct Apu {
    pub ch1: Channel1,
    pub ch2: Channel2,
    pub ch3: Channel3,
    pub ch4: Channel4,
    pub fifo_a: FifoChannel,
    pub fifo_b: FifoChannel,

    /// SOUNDCNT_L (0x04000080): NR50/NR51 equivalent — DMG stereo volume.
    /// Byte 0 (bits 7-0) = NR50 (master volume L/R + VIN).
    /// Byte 1 (bits 15-8) = NR51 (channel enable L/R).
    pub soundcnt_l: u16,

    /// SOUNDCNT_H (0x04000082): DMA sound control.
    /// bits 1-0: DMG output volume ratio (0=25%, 1=50%, 2=100%)
    /// bit 2: sound A volume (0=50%, 1=100%)
    /// bit 3: sound B volume (0=50%, 1=100%)
    /// bit 8: sound A right enable
    /// bit 9: sound A left enable
    /// bits 11-10: sound A timer select / DMA reset
    /// bit 12: sound B right enable
    /// bit 13: sound B left enable
    /// bits 15-14: sound B timer select / DMA reset
    pub soundcnt_h: u16,

    /// APU power flag (SOUNDCNT_X bit 7).
    pub powered: bool,

    /// SOUNDBIAS (0x04000088): PWM resolution and bias level.
    pub soundbias: u16,

    // ── Frame Sequencer ──────────────────────────────────────────────────
    fs_counter: u32,
    fs_step: u8,

    // ── Sample generation ────────────────────────────────────────────────
    sample_acc: f32,
    cycles_per_sample: f32,
    pending_sample: Option<(f32, f32)>,
}

impl Default for Apu {
    fn default() -> Self {
        Self::new()
    }
}

impl Apu {
    /// Create a new GBA APU in the powered-off state with a 44 100 Hz sample rate.
    pub fn new() -> Self {
        Self {
            ch1: Channel1::default(),
            ch2: Channel2::default(),
            ch3: Channel3::default(),
            ch4: Channel4::default(),
            fifo_a: FifoChannel::default(),
            fifo_b: FifoChannel::default(),
            soundcnt_l: 0,
            soundcnt_h: 0,
            powered: false,
            soundbias: 0x0200,
            fs_counter: 0,
            fs_step: 0,
            sample_acc: 0.0,
            cycles_per_sample: GBA_CLOCK_HZ / 44_100.0,
            pending_sample: None,
        }
    }

    // ── Public API ────────────────────────────────────────────────────────

    /// Set the output sample rate in Hz (default: 44 100).
    ///
    /// Invalid values (0, negative, NaN, infinity) are silently ignored.
    pub fn set_sample_rate(&mut self, rate: f32) {
        if !rate.is_finite() || rate <= 0.0 {
            return;
        }
        self.cycles_per_sample = GBA_CLOCK_HZ / rate;
    }

    /// Returns the configured output sample rate in Hz.
    pub fn sample_rate(&self) -> f32 {
        GBA_CLOCK_HZ / self.cycles_per_sample
    }

    /// Returns `true` when an output sample is ready to be collected.
    pub fn sample_ready(&self) -> bool {
        self.pending_sample.is_some()
    }

    /// Consume and return the pending audio sample as a mono downmix.
    ///
    /// Returns `(left + right) / 2` clamped to `[-1.0, 1.0]`, or `None` if no
    /// sample is pending.  The clamp guards against the sum of two values near
    /// ±1.0 slightly exceeding the range before halving.
    /// Prefer [`take_stereo_sample`](Apu::take_stereo_sample) for true stereo output.
    pub fn take_sample(&mut self) -> Option<f32> {
        self.pending_sample
            .take()
            .map(|(l, r)| ((l + r) / 2.0).clamp(-1.0, 1.0))
    }

    /// Consume and return the pending audio sample as a stereo `(left, right)` pair.
    ///
    /// Returns `None` if no sample is pending.
    pub fn take_stereo_sample(&mut self) -> Option<(f32, f32)> {
        self.pending_sample.take()
    }

    /// Advance the APU by `cycles` GBA CPU cycles.
    ///
    /// Rather than iterating one cycle at a time, this computes how many
    /// cycles remain until the next frame-sequencer event and the next sample
    /// boundary, advances the channel timers in one step, then handles those
    /// events in order. This keeps APU stepping O(events) instead of O(cycles).
    pub fn tick(&mut self, cycles: u32) {
        let mut remaining = cycles;
        while remaining > 0 {
            // How many cycles until the frame sequencer fires?
            let fs_remaining = FS_PERIOD - self.fs_counter;

            // How many cycles until the next output sample?
            // We track the fractional accumulator as f32; convert the ceiling
            // to an integer count of cycles needed.
            let sample_remaining = {
                let needed = self.cycles_per_sample - self.sample_acc;
                // Need at least 1 cycle, ceil to nearest integer.
                needed.ceil().max(1.0) as u32
            };

            // Advance only up to the nearest event (or end of budget).
            let step = remaining.min(fs_remaining).min(sample_remaining);

            // Bulk-advance channel frequency timers.
            self.ch1.tick(step);
            self.ch2.tick(step);
            self.ch3.tick(step);
            self.ch4.tick(step);

            // Advance frame sequencer counter.
            self.fs_counter += step;
            if self.fs_counter >= FS_PERIOD {
                self.fs_counter -= FS_PERIOD;
                self.clock_frame_sequencer_step();
            }

            // Advance sample accumulator.
            self.sample_acc += step as f32;
            if self.sample_acc >= self.cycles_per_sample {
                self.sample_acc -= self.cycles_per_sample;
                if self.pending_sample.is_none() {
                    self.pending_sample = Some(self.mix());
                }
            }

            remaining -= step;
        }
    }

    // ── Register read/write ───────────────────────────────────────────────

    /// Read a 16-bit APU register by absolute GBA I/O address.
    pub fn read16(&self, addr: u32) -> u16 {
        match addr {
            // SOUND1CNT_L (sweep)
            0x0400_0060 => {
                (self.ch1.sweep_shift as u16)
                    | (if self.ch1.sweep_negate { 0x08 } else { 0 })
                    | ((self.ch1.sweep_period as u16) << 4)
            }
            // SOUND1CNT_H (tone/envelope) — length write-only
            0x0400_0062 => {
                ((self.ch1.duty as u16) << 6)
                    | ((self.ch1.env_period as u16) << 8)
                    | (if self.ch1.env_add { 0x0800 } else { 0 })
                    | ((self.ch1.init_volume as u16) << 12)
            }
            // SOUND1CNT_X (freq/ctrl) — freq write-only, only length flag readable
            0x0400_0064 => {
                if self.ch1.length_en {
                    0x4000
                } else {
                    0
                }
            }
            // SOUND2CNT_L
            0x0400_0068 => {
                ((self.ch2.duty as u16) << 6)
                    | ((self.ch2.env_period as u16) << 8)
                    | (if self.ch2.env_add { 0x0800 } else { 0 })
                    | ((self.ch2.init_volume as u16) << 12)
            }
            // SOUND2CNT_H
            0x0400_006C => {
                if self.ch2.length_en {
                    0x4000
                } else {
                    0
                }
            }
            // SOUND3CNT_L
            0x0400_0070 => {
                (if self.ch3.two_banks { 0x0020 } else { 0 })
                    | (if self.ch3.bank_select { 0x0040 } else { 0 })
                    | (if self.ch3.dac_on { 0x0080 } else { 0 })
            }
            // SOUND3CNT_H
            0x0400_0072 => {
                ((self.ch3.output_level as u16) << 13)
                    | (if self.ch3.force_volume { 0x8000 } else { 0 })
            }
            // SOUND3CNT_X
            0x0400_0074 => {
                if self.ch3.length_en {
                    0x4000
                } else {
                    0
                }
            }
            // SOUND4CNT_L
            0x0400_0078 => {
                ((self.ch4.env_period as u16) << 8)
                    | (if self.ch4.env_add { 0x0800 } else { 0 })
                    | ((self.ch4.init_volume as u16) << 12)
            }
            // SOUND4CNT_H (clock/ctrl) — length write-only
            0x0400_007C => {
                (self.ch4.divisor_code as u16)
                    | (if self.ch4.lfsr_7bit { 0x08 } else { 0 })
                    | ((self.ch4.clock_shift as u16) << 4)
                    | (if self.ch4.length_en { 0x4000 } else { 0 })
            }
            // SOUNDCNT_L
            0x0400_0080 => self.soundcnt_l,
            // SOUNDCNT_H
            0x0400_0082 => self.soundcnt_h,
            // SOUNDCNT_X (status + power)
            0x0400_0084 => self.read_soundcnt_x(),
            // SOUNDBIAS
            0x0400_0088 => self.soundbias,
            // WAVE RAM (0x04000090-0x0400009F)
            a if (0x0400_0090..=0x0400_009E).contains(&a) && a & 1 == 0 => {
                let off = (a - 0x0400_0090) as usize;
                (self.ch3.read_wave_ram(off) as u16)
                    | ((self.ch3.read_wave_ram(off + 1) as u16) << 8)
            }
            _ => 0,
        }
    }

    /// Write a 16-bit APU register by absolute GBA I/O address.
    pub fn write16(&mut self, addr: u32, val: u16) {
        if !self.powered && (0x0400_0060..=0x0400_0081).contains(&addr) {
            return;
        }

        match addr {
            0x0400_0060 => self.ch1.write_cnt_l(val),
            0x0400_0062 => self.ch1.write_cnt_h(val),
            0x0400_0064 => self.ch1.write_cnt_x(val),
            0x0400_0068 => self.ch2.write_cnt_l(val),
            0x0400_006C => self.ch2.write_cnt_h(val),
            0x0400_0070 => self.ch3.write_cnt_l(val),
            0x0400_0072 => self.ch3.write_cnt_h(val),
            0x0400_0074 => self.ch3.write_cnt_x(val),
            0x0400_0078 => self.ch4.write_cnt_l(val),
            0x0400_007C => self.ch4.write_cnt_h(val),
            0x0400_0080 => self.soundcnt_l = val,
            0x0400_0082 => self.write_soundcnt_h(val),
            0x0400_0084 => self.write_soundcnt_x(val),
            0x0400_0088 => self.soundbias = val & 0xC3FE,
            // WAVE RAM halfword writes
            a if (0x0400_0090..=0x0400_009E).contains(&a) && a & 1 == 0 => {
                let off = (a - 0x0400_0090) as usize;
                self.ch3.write_wave_ram(off, val as u8);
                self.ch3.write_wave_ram(off + 1, (val >> 8) as u8);
            }
            // FIFO_A write: 0x00A0–0x00A3 (all four bytes map to FIFO A)
            0x0400_00A0 | 0x0400_00A2 => {
                let lo = val as u8;
                let hi = (val >> 8) as u8;
                self.fifo_a.push(lo as i8);
                self.fifo_a.push(hi as i8);
            }
            // FIFO_B write: 0x00A4–0x00A7
            0x0400_00A4 | 0x0400_00A6 => {
                let lo = val as u8;
                let hi = (val >> 8) as u8;
                self.fifo_b.push(lo as i8);
                self.fifo_b.push(hi as i8);
            }
            _ => {}
        }
    }

    /// Write a single byte to an APU register.
    ///
    /// Many APU register fields are write-only (length, trigger, frequency),
    /// so the standard read-modify-write pattern would corrupt them. This
    /// method therefore routes byte writes to dedicated single-byte handlers
    /// for every register that contains write-only fields, avoiding spurious
    /// reads through `read16`.
    pub fn write8(&mut self, addr: u32, val: u8) {
        if !self.powered && (0x0400_0060..=0x0400_0081).contains(&addr) {
            return;
        }

        match addr {
            // SOUND1CNT_L: both bytes are fully read/write — RMW is safe.
            0x0400_0060 | 0x0400_0061 => {
                let aligned = addr & !0x1;
                let current = self.read16(aligned);
                let merged = if addr & 1 == 0 {
                    (current & 0xFF00) | val as u16
                } else {
                    (current & 0x00FF) | ((val as u16) << 8)
                };
                self.ch1.write_cnt_l(merged);
            }
            // SOUND1CNT_H: high byte (envelope/volume) is read/write;
            // low byte contains write-only length field — handle separately.
            0x0400_0062 => {
                // Low byte: duty (7-6) + length (5-0).  Length is write-only,
                // so we cannot RMW — write the full low byte directly.
                let current_hi = self.read16(0x0400_0062) & 0xFF00;
                self.ch1.write_cnt_h(current_hi | val as u16);
            }
            0x0400_0063 => {
                // High byte: envelope period, add, init volume — read/write.
                let current_lo = self.read16(0x0400_0062) & 0x00FF;
                self.ch1.write_cnt_h(current_lo | ((val as u16) << 8));
            }
            // SOUND1CNT_X: frequency (10-0) and length-enable are write-only;
            // trigger bit must be passed through.
            0x0400_0064 => {
                // Low byte: low 8 bits of frequency — write-only.
                let hi = (self.ch1.freq >> 8) & 0x07;
                let len_en = u16::from(self.ch1.length_en) << 6;
                self.ch1.write_cnt_x(len_en | (hi << 8) | val as u16);
            }
            0x0400_0065 => {
                // High byte: freq[10:8] | length_en | trigger.
                self.ch1
                    .write_cnt_x((val as u16) << 8 | (self.ch1.freq & 0xFF));
            }
            // SOUND2CNT_L
            0x0400_0068 => {
                let current_hi = self.read16(0x0400_0068) & 0xFF00;
                self.ch2.write_cnt_l(current_hi | val as u16);
            }
            0x0400_0069 => {
                let current_lo = self.read16(0x0400_0068) & 0x00FF;
                self.ch2.write_cnt_l(current_lo | ((val as u16) << 8));
            }
            // SOUND2CNT_H
            0x0400_006C => {
                let hi = (self.ch2.freq >> 8) & 0x07;
                let len_en = u16::from(self.ch2.length_en) << 6;
                self.ch2.write_cnt_h(len_en | (hi << 8) | val as u16);
            }
            0x0400_006D => {
                self.ch2
                    .write_cnt_h((val as u16) << 8 | (self.ch2.freq & 0xFF));
            }
            // SOUND3CNT_L: DAC enable in bit 7
            0x0400_0070 => {
                self.ch3.write_cnt_l(val as u16);
            }
            0x0400_0071 => {} // high byte unused
            // SOUND3CNT_H: length (7-0) in low byte, output level (14-13) in high byte
            0x0400_0072 => {
                let current_hi = self.read16(0x0400_0072) & 0xFF00;
                self.ch3.write_cnt_h(current_hi | val as u16);
            }
            0x0400_0073 => {
                // High byte contains output level + force-volume and must not clobber
                // the write-only length value in low byte.
                self.ch3.output_level = (val >> 5) & 0x03;
                self.ch3.force_volume = (val & 0x80) != 0;
            }
            // SOUND3CNT_X: frequency/trigger
            0x0400_0074 => {
                let hi = (self.ch3.freq >> 8) & 0x07;
                let len_en = u16::from(self.ch3.length_en) << 6;
                self.ch3.write_cnt_x(len_en | (hi << 8) | val as u16);
            }
            0x0400_0075 => {
                self.ch3
                    .write_cnt_x((val as u16) << 8 | (self.ch3.freq & 0xFF));
            }
            // SOUND4CNT_L: length (5-0) in low byte, envelope in high byte
            0x0400_0078 => {
                let current_hi = self.read16(0x0400_0078) & 0xFF00;
                self.ch4.write_cnt_l(current_hi | val as u16);
            }
            0x0400_0079 => {
                let current_lo = self.read16(0x0400_0078) & 0x00FF;
                self.ch4.write_cnt_l(current_lo | ((val as u16) << 8));
            }
            // SOUND4CNT_H: clock params + trigger
            0x0400_007C => {
                let len_en = u16::from(self.ch4.length_en) << 6;
                self.ch4
                    .write_cnt_h(len_en | (0xFF00 & self.read16(0x0400_007C)) | val as u16);
            }
            0x0400_007D => {
                self.ch4
                    .write_cnt_h((val as u16) << 8 | (self.read16(0x0400_007C) & 0x00FF));
            }
            // SOUNDCNT_L: plain read/write register — RMW safe.
            0x0400_0080 => {
                self.soundcnt_l = (self.soundcnt_l & 0xFF00) | val as u16;
            }
            0x0400_0081 => {
                self.soundcnt_l = (self.soundcnt_l & 0x00FF) | ((val as u16) << 8);
            }
            // SOUNDCNT_H: DMA sound control — use write16 (handles FIFO reset).
            0x0400_0082 => {
                let hi = self.soundcnt_h & 0xFF00;
                self.write_soundcnt_h(hi | val as u16);
            }
            0x0400_0083 => {
                let lo = self.soundcnt_h & 0x00FF;
                self.write_soundcnt_h(lo | ((val as u16) << 8));
            }
            // SOUNDCNT_X: power control — use write16.
            0x0400_0084 => self.write_soundcnt_x(val as u16),
            0x0400_0085 => {} // high byte of SOUNDCNT_X is unused
            // SOUNDBIAS
            0x0400_0088 => {
                let hi = self.soundbias & 0xFF00;
                self.soundbias = (hi | val as u16) & 0xC3FE;
            }
            0x0400_0089 => {
                let lo = self.soundbias & 0x00FF;
                self.soundbias = (lo | ((val as u16) << 8)) & 0xC3FE;
            }
            // WAVE RAM: byte-addressable
            a if (0x0400_0090..=0x0400_009F).contains(&a) => {
                let off = (a - 0x0400_0090) as usize;
                self.ch3.write_wave_ram(off, val);
            }
            // FIFO A (0x00A0–0x00A3): each byte is a new PCM sample
            a if (0x0400_00A0..=0x0400_00A3).contains(&a) => {
                self.fifo_a.push(val as i8);
            }
            // FIFO B (0x00A4–0x00A7)
            a if (0x0400_00A4..=0x0400_00A7).contains(&a) => {
                self.fifo_b.push(val as i8);
            }
            _ => {}
        }
    }

    /// Write a 32-bit word (4 bytes) to FIFO A.
    pub fn write_fifo_a_word(&mut self, val: u32) {
        self.fifo_a.push_word(val);
    }

    /// Write a 32-bit word (4 bytes) to FIFO B.
    pub fn write_fifo_b_word(&mut self, val: u32) {
        self.fifo_b.push_word(val);
    }

    /// Push a single 8-bit signed sample directly into FIFO A (for testing).
    pub fn push_fifo_a(&mut self, sample: i8) {
        self.fifo_a.push(sample);
    }

    /// Push a single 8-bit signed sample directly into FIFO B (for testing).
    pub fn push_fifo_b(&mut self, sample: i8) {
        self.fifo_b.push(sample);
    }

    // ── Internal helpers ──────────────────────────────────────────────────

    fn read_soundcnt_x(&self) -> u16 {
        let mut val: u16 = if self.powered { 0x0080 } else { 0 };
        if self.ch1.active {
            val |= 0x01;
        }
        if self.ch2.active {
            val |= 0x02;
        }
        if self.ch3.active {
            val |= 0x04;
        }
        if self.ch4.active {
            val |= 0x08;
        }
        val
    }

    fn write_soundcnt_h(&mut self, val: u16) {
        self.soundcnt_h = val;
        // FIFO A reset (bit 11) — auto-clear after triggering.
        if val & 0x0800 != 0 {
            self.fifo_a.clear();
            self.soundcnt_h &= !0x0800;
        }
        // FIFO B reset (bit 15) — auto-clear after triggering.
        if val & 0x8000 != 0 {
            self.fifo_b.clear();
            self.soundcnt_h &= !0x8000;
        }
    }

    fn write_soundcnt_x(&mut self, val: u16) {
        let new_power = val & 0x0080 != 0;
        if !new_power && self.powered {
            // Power-off: clear all channel registers.
            self.ch1.power_off();
            self.ch2.power_off();
            self.ch3.power_off();
            self.ch4.power_off();
            self.soundcnt_l = 0;
            // SOUNDCNT_H and FIFO channels are retained.
        }
        self.powered = new_power;
    }

    /// Clock one frame sequencer step manually (for testing).
    pub fn clock_frame_sequencer_step(&mut self) {
        let flags = FS_TABLE[self.fs_step as usize];
        if flags & 0x01 != 0 {
            self.ch1.clock_length();
            self.ch2.clock_length();
            self.ch3.clock_length();
            self.ch4.clock_length();
        }
        if flags & 0x02 != 0 {
            self.ch1.clock_sweep();
        }
        if flags & 0x04 != 0 {
            self.ch1.clock_envelope();
            self.ch2.clock_envelope();
            self.ch4.clock_envelope();
        }
        self.fs_step = (self.fs_step + 1) & 7;
    }

    /// Mix all channels into a stereo `(left, right)` pair in `[-1.0, 1.0]`.
    fn mix(&self) -> (f32, f32) {
        if !self.powered {
            // PCM channels are still active even when SOUNDCNT_X power is off.
            return self.mix_pcm_only();
        }

        // ── DMG channels ─────────────────────────────────────────────────
        let nr50 = (self.soundcnt_l & 0xFF) as u8;
        let nr51 = (self.soundcnt_l >> 8) as u8;

        let dmg_samples = [
            self.ch1.output(),
            self.ch2.output(),
            self.ch3.output(),
            self.ch4.output(),
        ];

        // NR51 bits 3-0 = right enables (CH1-CH4), bits 7-4 = left enables.
        let right_mix = Self::mix_terminal(dmg_samples, nr51 & 0x0F);
        let left_mix = Self::mix_terminal(dmg_samples, nr51 >> 4);

        // NR50 bits 2-0 = right vol (0-7), bits 6-4 = left vol.
        let right_vol = (nr50 & 0x07) as f32 / 7.0;
        let left_vol = ((nr50 >> 4) & 0x07) as f32 / 7.0;

        // SOUNDCNT_H bits 1-0: DMG volume ratio (0=25%, 1=50%, 2=100%).
        let dmg_ratio = match self.soundcnt_h & 0x03 {
            0 => 0.25,
            1 => 0.5,
            _ => 1.0,
        };

        let dmg_right = right_mix * right_vol * dmg_ratio;
        let dmg_left = left_mix * left_vol * dmg_ratio;

        // ── PCM channels ─────────────────────────────────────────────────
        let pcm_a_vol = if self.soundcnt_h & 0x04 != 0 {
            1.0
        } else {
            0.5
        };
        let pcm_b_vol = if self.soundcnt_h & 0x08 != 0 {
            1.0
        } else {
            0.5
        };

        let a_right = if self.soundcnt_h & 0x0100 != 0 {
            self.fifo_a.output() * pcm_a_vol
        } else {
            0.0
        };
        let a_left = if self.soundcnt_h & 0x0200 != 0 {
            self.fifo_a.output() * pcm_a_vol
        } else {
            0.0
        };
        let b_right = if self.soundcnt_h & 0x1000 != 0 {
            self.fifo_b.output() * pcm_b_vol
        } else {
            0.0
        };
        let b_left = if self.soundcnt_h & 0x2000 != 0 {
            self.fifo_b.output() * pcm_b_vol
        } else {
            0.0
        };

        let total_right = (dmg_right + a_right + b_right).clamp(-1.0, 1.0);
        let total_left = (dmg_left + a_left + b_left).clamp(-1.0, 1.0);

        (total_left, total_right)
    }

    /// Mix only PCM channels (used when APU power is off).
    fn mix_pcm_only(&self) -> (f32, f32) {
        let pcm_a_vol = if self.soundcnt_h & 0x04 != 0 {
            1.0
        } else {
            0.5
        };
        let pcm_b_vol = if self.soundcnt_h & 0x08 != 0 {
            1.0
        } else {
            0.5
        };

        let a_right = if self.soundcnt_h & 0x0100 != 0 {
            self.fifo_a.output() * pcm_a_vol
        } else {
            0.0
        };
        let a_left = if self.soundcnt_h & 0x0200 != 0 {
            self.fifo_a.output() * pcm_a_vol
        } else {
            0.0
        };
        let b_right = if self.soundcnt_h & 0x1000 != 0 {
            self.fifo_b.output() * pcm_b_vol
        } else {
            0.0
        };
        let b_left = if self.soundcnt_h & 0x2000 != 0 {
            self.fifo_b.output() * pcm_b_vol
        } else {
            0.0
        };

        let total_right = (a_right + b_right).clamp(-1.0, 1.0);
        let total_left = (a_left + b_left).clamp(-1.0, 1.0);

        (total_left, total_right)
    }

    /// Mix DMG channel samples through a 4-bit enable mask.
    fn mix_terminal(samples: [f32; 4], enable_mask: u8) -> f32 {
        samples
            .iter()
            .enumerate()
            .map(|(i, &s)| if enable_mask & (1 << i) != 0 { s } else { 0.0 })
            .sum::<f32>()
            / 4.0
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helper ──────────────────────────────────────────────────────────────

    /// Create a powered-on APU.
    fn powered_apu() -> Apu {
        let mut apu = Apu::new();
        apu.write16(0x0400_0084, 0x0080); // SOUNDCNT_X: power on
        apu
    }

    // ── PCM channel A test (issue test vector) ───────────────────────────────

    #[test]
    fn test_pcm_channel_a_sample_output() {
        let mut apu = powered_apu();

        // Enable FIFO A: full volume (bit 2), right (bit 8) + left (bit 9).
        // SOUNDCNT_H = 0x0304
        apu.soundcnt_h = 0x0304;

        let pcm_samples: [i8; 8] = [64, -64, 32, -32, 16, -16, 8, -8];
        for &s in &pcm_samples {
            apu.push_fifo_a(s);
        }

        // For each PCM sample, force one mixer call (advance FIFO + mix) and verify.
        for &expected_pcm in &pcm_samples {
            // Advance FIFO to consume the next sample, then mix.
            apu.fifo_a.advance();
            let (left, right) = apu.mix();

            // Both L and R are enabled at equal volume, so they should be equal.
            let expected = expected_pcm as f32 / 128.0; // full-volume, both L+R equal
            assert!(
                (left - expected).abs() < 1e-5,
                "PCM A left, sample {expected_pcm}: expected {expected:.4}, got {left:.4}"
            );
            assert!(
                (right - expected).abs() < 1e-5,
                "PCM A right, sample {expected_pcm}: expected {expected:.4}, got {right:.4}"
            );
        }
    }

    // ── CH1 sweep test (issue test vector) ──────────────────────────────────

    #[test]
    fn test_ch1_sweep_decreases_frequency() {
        let mut apu = powered_apu();

        // SOUND1CNT_L: sweep period=2, decrease (bit 3=1), shift=1 → 0x29
        // Bits 6-4=010 (period), bit 3=1 (negate), bits 2-0=001 (shift)
        // = 0b0_010_1_001 = 0x29
        apu.write16(0x0400_0060, 0x0029);

        // SOUND1CNT_H: duty=2 (50%), volume=15, env off → 0xF080
        apu.write16(0x0400_0062, 0xF080);

        // SOUND1CNT_X: freq=800, trigger → 0x8320
        apu.write16(0x0400_0064, 0x8320);

        let initial_shadow = apu.ch1.sweep_shadow;
        assert_eq!(initial_shadow, 800);

        // Frame sequencer sweep fires at steps 2 and 6.
        // For period=2, we need 2 sweep clock events to fire once.
        // Clock FS steps 0-6 (7 steps) to reach step 6 where 2nd sweep fires.
        // step 0: length only
        // step 2: sweep → timer 2→1
        // step 6: sweep → timer 1→0 → fire!
        for _ in 0..7 {
            apu.clock_frame_sequencer_step();
        }

        let expected_new_freq = 800 - (800_u16 >> 1); // = 400
        assert_eq!(
            apu.ch1.sweep_shadow, expected_new_freq,
            "Sweep should decrease frequency from 800 to 400 (shift=1, negate)"
        );
        assert_eq!(apu.ch1.freq, expected_new_freq);
    }

    #[test]
    fn test_ch1_sweep_disabled_when_period_zero() {
        let mut apu = powered_apu();
        // sweep period=0 → disabled
        apu.write16(0x0400_0060, 0x0001); // shift=1, period=0
        apu.write16(0x0400_0062, 0xF080);
        apu.write16(0x0400_0064, 0x8320); // freq=800, trigger

        let initial_shadow = apu.ch1.sweep_shadow;
        // Clock many FS steps
        for _ in 0..16 {
            apu.clock_frame_sequencer_step();
        }
        // Frequency should not change when period=0
        assert_eq!(apu.ch1.sweep_shadow, initial_shadow);
    }

    // ── CH2 pulse test ──────────────────────────────────────────────────────

    #[test]
    fn test_ch2_trigger_activates_channel() {
        let mut apu = powered_apu();
        // SOUND2CNT_L: volume=15 (bits 15-12), env add=0, env period=0, duty=2
        apu.write16(0x0400_0068, 0xF080);
        // SOUND2CNT_H: freq=1000, trigger
        apu.write16(0x0400_006C, 0x83E8);
        assert!(apu.ch2.active);
    }

    #[test]
    fn test_ch2_length_counter_stops_channel() {
        let mut apu = powered_apu();
        // length_counter = 64 - 63 = 1, duty=2, vol=15
        apu.write16(0x0400_0068, 0xF0BF); // length=63 → counter=1
        // trigger + length enable
        apu.write16(0x0400_006C, 0xC3E8); // freq=1000, length_en=1, trigger
        assert!(apu.ch2.active);
        apu.clock_frame_sequencer_step(); // step 0: clock length
        assert!(
            !apu.ch2.active,
            "Channel should stop when length counter expires"
        );
    }

    #[test]
    fn test_ch2_envelope_decrements_volume() {
        let mut apu = powered_apu();
        // volume=7, env_period=1, env_sub (decrease)
        apu.write16(0x0400_0068, 0x7100); // vol=7, env_period=1, env_add=0
        apu.write16(0x0400_006C, 0x83E8);
        let vol_before = apu.ch2.volume;
        // step 7: envelope
        for _ in 0..8 {
            apu.clock_frame_sequencer_step();
        }
        assert!(
            apu.ch2.volume < vol_before,
            "Volume should decrease after envelope step"
        );
    }

    // ── CH3 wave test (issue test vector) ───────────────────────────────────

    #[test]
    fn test_ch3_wave_output_matches_table() {
        let mut apu = powered_apu();

        // Load a known wave table: nibbles 0x0,0x1,...,0xF repeated
        // Bytes: 0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF × 2
        let wave: [u8; 16] = [
            0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67, 0x89, 0xAB,
            0xCD, 0xEF,
        ];
        // Write wave RAM via register interface.
        // With bank_select=0 (default), writes go to bank 1 (the other bank).
        for (i, &b) in wave.iter().enumerate().step_by(2) {
            let hw = (b as u16) | ((wave[i + 1] as u16) << 8);
            apu.write16(0x0400_0090 + i as u32, hw);
        }

        // SOUND3CNT_L: bank_select=1 (bit 6) + DAC on (bit 7).
        // Switching bank_select to 1 makes bank 1 the playing bank, so the data
        // written above (which went to bank 1 as the "other bank") is now played.
        apu.write16(0x0400_0070, 0x00C0);
        // SOUND3CNT_H: output level = 100% (bits 14-13 = 01 = 0x2000), length = 0
        apu.write16(0x0400_0072, 0x2000);
        // SOUND3CNT_X: freq=0 (slow), trigger
        apu.write16(0x0400_0074, 0x8000);

        assert!(apu.ch3.active, "Channel 3 should be active after trigger");

        // After trigger: wave_pos=0, first nibble = high nibble of byte 0 = 0x0
        // Tick just past one period ((2048-0)×8 = 16384 GBA cycles) to advance wave_pos to 1
        let period_per_step = (2048_u32) * 8; // freq=0

        // Collect first 8 nibbles by advancing one step at a time.
        // The current_sample after trigger is nibble at wave_pos=0 (high nibble of byte 0) = 0x0.
        // After one step, wave_pos=1 → low nibble of byte 0 = 0x1.
        let expected_nibbles: [u8; 8] = [0x0, 0x1, 0x2, 0x3, 0x4, 0x5, 0x6, 0x7];

        // First nibble is set at trigger time.
        assert_eq!(
            apu.ch3.current_sample, expected_nibbles[0],
            "Initial sample should be 0x0"
        );

        for &expected in &expected_nibbles[1..] {
            apu.ch3.tick(period_per_step);
            assert_eq!(
                apu.ch3.current_sample, expected,
                "Wave nibble mismatch: expected 0x{expected:X}, got 0x{:X}",
                apu.ch3.current_sample
            );
        }
    }

    // ── CH3 force-volume tests (GBATek SOUND3CNT_H bit 15) ──────────────────

    #[test]
    fn test_ch3_force_volume_bit_sets_field() {
        // RED: bit 15 of SOUND3CNT_H must set force_volume on the channel.
        let mut apu = powered_apu();
        apu.write16(0x0400_0072, 0x8000); // bit 15 set
        assert!(
            apu.ch3.force_volume,
            "force_volume must be true when SOUND3CNT_H bit 15 is set"
        );
    }

    #[test]
    fn test_ch3_force_volume_false_when_bit15_clear() {
        let mut apu = powered_apu();
        apu.write16(0x0400_0072, 0x2000); // output_level=100%, no force
        assert!(
            !apu.ch3.force_volume,
            "force_volume must be false when SOUND3CNT_H bit 15 is clear"
        );
    }

    #[test]
    fn test_write8_sound3cnt_h_high_byte_does_not_clobber_length() {
        let mut apu = powered_apu();
        apu.write8(0x0400_0072, 0x20); // length = 32 -> counter = 224
        let before = apu.ch3.length_counter;

        apu.write8(0x0400_0073, 0xC0); // force_volume=1, output_level=2

        assert_eq!(
            apu.ch3.length_counter, before,
            "high-byte write must not change SOUND3CNT_H length counter"
        );
        assert_eq!(apu.ch3.output_level, 2);
        assert!(apu.ch3.force_volume);
    }

    #[test]
    fn test_ch3_force_volume_read_back() {
        // Bit 15 written to SOUND3CNT_H must be readable back.
        let mut apu = powered_apu();
        apu.write16(0x0400_0072, 0xA000); // bit 15 + output_level=100% (bits 14-13=01)
        let readback = apu.read16(0x0400_0072);
        assert_eq!(
            readback & 0x8000,
            0x8000,
            "Bit 15 (force_volume) must read back as set"
        );
        assert_eq!(
            readback & 0x6000,
            0x2000,
            "output_level bits must also read back correctly"
        );
    }

    // Per GBATek (gbatek-gba-sound-channel-4-noise.htm):
    //   15-bit mode initial LFSR: X = 0x4000
    //   7-bit mode  initial LFSR: X = 0x0040
    //   Algorithm (Galois form):
    //     X = X SHR 1; IF carry THEN X = X XOR 6000h (15-bit) or X XOR 0060h (7-bit)

    #[test]
    fn test_ch4_15bit_trigger_initial_lfsr_is_4000() {
        // RED: After trigger in 15-bit mode, LFSR must be 0x4000 per GBATek.
        let mut apu = powered_apu();
        apu.write16(0x0400_0078, 0xF000); // volume=15
        apu.write16(0x0400_007C, 0x8000); // 15-bit mode (bit3=0), trigger
        assert_eq!(
            apu.ch4.lfsr, 0x4000,
            "GBATek: 15-bit LFSR initial value should be 0x4000"
        );
    }

    #[test]
    fn test_ch4_7bit_trigger_initial_lfsr_is_40() {
        // RED: After trigger in 7-bit mode, LFSR must be 0x0040 per GBATek.
        let mut apu = powered_apu();
        apu.write16(0x0400_0078, 0xF000); // volume=15
        apu.write16(0x0400_007C, 0x8008); // 7-bit mode (bit3=1), trigger
        assert_eq!(
            apu.ch4.lfsr, 0x0040,
            "GBATek: 7-bit LFSR initial value should be 0x0040"
        );
    }

    #[test]
    fn test_ch4_lfsr_sequence_15bit() {
        let mut apu = powered_apu();

        // SOUND4CNT_L: volume=15, env add=0, env_period=0
        apu.write16(0x0400_0078, 0xF000);
        // SOUND4CNT_H: divisor=0 (r=0), shift=0, 15-bit mode (bit 3=0), trigger
        apu.write16(0x0400_007C, 0x8000);

        // After trigger, LFSR is 0x4000 per GBATek.
        assert_eq!(apu.ch4.lfsr, 0x4000);

        // Compute expected LFSR sequence using GBATek Galois form.
        // clock_lfsr: carry = lfsr & 1; lfsr >>= 1; if carry { lfsr ^= 0x6000 }
        let mut lfsr = 0x4000_u16;
        fn clock(lfsr: &mut u16) {
            let carry = *lfsr & 1;
            *lfsr >>= 1;
            if carry != 0 {
                *lfsr ^= 0x6000;
            }
        }

        // Clock LFSR 8 times, verify sequence matches channel.
        for _ in 0..8 {
            clock(&mut lfsr);
            apu.ch4.clock_lfsr();
            assert_eq!(
                apu.ch4.lfsr, lfsr,
                "LFSR mismatch after clock: expected 0x{lfsr:04X}, got 0x{:04X}",
                apu.ch4.lfsr
            );
        }
    }

    // ── sample_ready / get_sample / set_sample_rate ──────────────────────────

    #[test]
    fn test_sample_ready_after_enough_ticks() {
        let mut apu = Apu::new();
        apu.set_sample_rate(44_100.0);
        let cycles = (GBA_CLOCK_HZ / 44_100.0) as u32 + 1;
        apu.tick(cycles);
        assert!(
            apu.sample_ready(),
            "A sample should be ready after one sample period"
        );
    }

    #[test]
    fn test_get_sample_consumes_pending() {
        let mut apu = Apu::new();
        apu.set_sample_rate(44_100.0);
        let cycles = (GBA_CLOCK_HZ / 44_100.0) as u32 + 1;
        apu.tick(cycles);
        assert!(apu.sample_ready());
        let s = apu.take_sample();
        assert!(s.is_some());
        assert!(!apu.sample_ready());
    }

    #[test]
    fn test_get_sample_range() {
        let mut apu = powered_apu();
        apu.set_sample_rate(44_100.0);

        // Set up a loud CH1 signal
        apu.write16(0x0400_0060, 0x0000);
        apu.write16(0x0400_0062, 0xF080);
        apu.write16(0x0400_0064, 0x8320);

        // SOUNDCNT_L: full L+R volume, all channels enabled
        apu.write16(0x0400_0080, 0xFF77);
        // SOUNDCNT_H: DMG at 100%
        apu.soundcnt_h = 0x0002;

        let cycles = (GBA_CLOCK_HZ / 44_100.0) as u32 + 1;
        apu.tick(cycles);
        if let Some(s) = apu.take_sample() {
            assert!(
                (-1.0..=1.0).contains(&s),
                "Sample must be in [-1.0, 1.0], got {s}"
            );
        }
    }

    #[test]
    fn test_set_audio_sample_rate_adjusts_timing() {
        let mut apu = Apu::new();
        apu.set_sample_rate(22_050.0);
        let cycles = (GBA_CLOCK_HZ / 22_050.0) as u32 + 1;
        apu.tick(cycles);
        assert!(
            apu.sample_ready(),
            "Sample should be ready at 22 050 Hz rate"
        );
    }

    // ── Power control ────────────────────────────────────────────────────────

    #[test]
    fn test_power_off_clears_channels() {
        let mut apu = powered_apu();
        apu.write16(0x0400_0062, 0xF080);
        apu.write16(0x0400_0064, 0x8320);
        assert!(apu.ch1.active);

        // Power off
        apu.write16(0x0400_0084, 0x0000);
        assert!(!apu.powered);
        assert!(!apu.ch1.active);
    }

    #[test]
    fn test_psg_write16_ignored_when_powered_off() {
        let mut apu = powered_apu();
        apu.write16(0x0400_0060, 0x0077);
        apu.write16(0x0400_0080, 0xABCD);
        assert_ne!(apu.read16(0x0400_0060), 0);
        assert_ne!(apu.read16(0x0400_0080), 0);

        apu.write16(0x0400_0084, 0x0000); // power off clears PSG regs
        assert_eq!(apu.read16(0x0400_0060), 0);
        assert_eq!(apu.read16(0x0400_0080), 0);

        apu.write16(0x0400_0060, 0x0066);
        apu.write16(0x0400_0080, 0x1234);
        assert_eq!(apu.read16(0x0400_0060), 0);
        assert_eq!(apu.read16(0x0400_0080), 0);
    }

    #[test]
    fn test_psg_write8_ignored_when_powered_off() {
        let mut apu = powered_apu();
        apu.write8(0x0400_0060, 0x55);
        apu.write8(0x0400_0080, 0x34);
        apu.write8(0x0400_0081, 0x12);
        assert_ne!(apu.read16(0x0400_0060), 0);
        assert_ne!(apu.read16(0x0400_0080), 0);

        apu.write16(0x0400_0084, 0x0000); // power off clears PSG regs
        assert_eq!(apu.read16(0x0400_0060), 0);
        assert_eq!(apu.read16(0x0400_0080), 0);

        apu.write8(0x0400_0060, 0x66);
        apu.write8(0x0400_0080, 0x78);
        apu.write8(0x0400_0081, 0x56);
        assert_eq!(apu.read16(0x0400_0060), 0);
        assert_eq!(apu.read16(0x0400_0080), 0);
    }

    #[test]
    fn test_soundcnt_h_and_soundbias_stay_writable_when_powered_off() {
        let mut apu = Apu::new();
        assert!(!apu.powered);

        apu.write16(0x0400_0082, 0x030F);
        apu.write16(0x0400_0088, 0xC3FE);

        assert_eq!(apu.read16(0x0400_0082), 0x030F);
        assert_eq!(apu.read16(0x0400_0088), 0xC3FE);

        apu.write8(0x0400_0082, 0xAA);
        apu.write8(0x0400_0083, 0x55);
        apu.write8(0x0400_0088, 0x34);
        apu.write8(0x0400_0089, 0x12);

        assert_eq!(apu.read16(0x0400_0082), 0x55AA);
        assert_eq!(apu.read16(0x0400_0088), 0x0234);
    }

    // ── Register round-trip tests ────────────────────────────────────────────

    #[test]
    fn test_soundcnt_x_power_bit_readable() {
        let mut apu = Apu::new();
        apu.write16(0x0400_0084, 0x0080);
        assert_eq!(apu.read16(0x0400_0084) & 0x0080, 0x0080);
        apu.write16(0x0400_0084, 0x0000);
        assert_eq!(apu.read16(0x0400_0084) & 0x0080, 0x0000);
    }

    #[test]
    fn test_soundcnt_l_round_trips() {
        let mut apu = powered_apu();
        apu.write16(0x0400_0080, 0xABCD);
        assert_eq!(apu.read16(0x0400_0080), 0xABCD);
    }

    #[test]
    fn test_wave_ram_round_trips() {
        let mut apu = Apu::new();
        apu.write16(0x0400_0090, 0xDEAD);
        let val = apu.read16(0x0400_0090);
        assert_eq!(val, 0xDEAD);
    }

    // ── set_sample_rate validation ───────────────────────────────────────────

    #[test]
    fn test_set_sample_rate_rejects_zero() {
        let mut apu = Apu::new();
        let before = apu.cycles_per_sample;
        apu.set_sample_rate(0.0);
        assert_eq!(apu.cycles_per_sample, before, "zero rate should be ignored");
    }

    #[test]
    fn test_set_sample_rate_rejects_negative() {
        let mut apu = Apu::new();
        let before = apu.cycles_per_sample;
        apu.set_sample_rate(-100.0);
        assert_eq!(
            apu.cycles_per_sample, before,
            "negative rate should be ignored"
        );
    }

    #[test]
    fn test_set_sample_rate_rejects_nan() {
        let mut apu = Apu::new();
        let before = apu.cycles_per_sample;
        apu.set_sample_rate(f32::NAN);
        assert_eq!(apu.cycles_per_sample, before, "NaN rate should be ignored");
    }

    #[test]
    fn test_set_sample_rate_rejects_infinity() {
        let mut apu = Apu::new();
        let before = apu.cycles_per_sample;
        apu.set_sample_rate(f32::INFINITY);
        assert_eq!(
            apu.cycles_per_sample, before,
            "infinity rate should be ignored"
        );
    }

    // ── write8 tests ─────────────────────────────────────────────────────────

    #[test]
    fn test_write8_fifo_a_pushes_each_byte() {
        let mut apu = Apu::new();
        // Each byte address 0xA0–0xA3 should push one sample.
        apu.write8(0x0400_00A0, 10);
        apu.write8(0x0400_00A1, 20);
        apu.write8(0x0400_00A2, 30);
        apu.write8(0x0400_00A3, 40);
        assert_eq!(apu.fifo_a.len(), 4);
    }

    #[test]
    fn test_write8_fifo_b_pushes_each_byte() {
        let mut apu = Apu::new();
        apu.write8(0x0400_00A4, 1);
        apu.write8(0x0400_00A5, 2);
        apu.write8(0x0400_00A6, 3);
        apu.write8(0x0400_00A7, 4);
        assert_eq!(apu.fifo_b.len(), 4);
    }

    #[test]
    fn test_write8_wave_ram_byte_addressable() {
        let mut apu = Apu::new();
        // bank_select=0 (default): writes go to the other bank (bank 1).
        apu.write8(0x0400_0090, 0xAB);
        apu.write8(0x0400_0091, 0xCD);
        assert_eq!(apu.ch3.wave_ram[1][0], 0xAB);
        assert_eq!(apu.ch3.wave_ram[1][1], 0xCD);
    }

    #[test]
    fn test_write8_soundcnt_l_preserves_other_byte() {
        let mut apu = powered_apu();
        apu.write16(0x0400_0080, 0x1234);
        // Overwrite only the low byte.
        apu.write8(0x0400_0080, 0x56);
        assert_eq!(apu.soundcnt_l, 0x1256, "high byte should be preserved");
        // Overwrite only the high byte.
        apu.write8(0x0400_0081, 0x78);
        assert_eq!(apu.soundcnt_l, 0x7856, "low byte should be preserved");
    }

    // ── Stereo output tests ──────────────────────────────────────────────────

    #[test]
    fn test_take_stereo_sample_returns_pair() {
        // take_stereo_sample() must return Some((left, right)) after enough ticks.
        let mut apu = Apu::new();
        apu.set_sample_rate(44_100.0);
        let cycles = (GBA_CLOCK_HZ / 44_100.0) as u32 + 1;
        apu.tick(cycles);
        assert!(apu.sample_ready(), "sample should be ready");
        let stereo = apu.take_stereo_sample();
        assert!(stereo.is_some(), "take_stereo_sample() must return Some");
        assert!(
            !apu.sample_ready(),
            "sample should be consumed after take_stereo_sample"
        );
    }

    #[test]
    fn test_stereo_mix_ch1_left_only() {
        // When CH1 is enabled on LEFT only (NR51 bit 4), right channel must be 0.
        let mut apu = powered_apu();
        // NR50 (low byte) = 0x77 (full L+R vol), NR51 (high byte) = 0x10 (CH1 LEFT only).
        apu.write16(0x0400_0080, 0x1077);
        // SOUNDCNT_H: DMG at 100% (bits 1-0 = 2)
        apu.soundcnt_h = 0x0002;
        // Trigger CH1 with duty=50%, volume=15, freq=800.
        apu.write16(0x0400_0062, 0xF080);
        apu.write16(0x0400_0064, 0x8320);

        let (left, right) = apu.mix();
        assert_eq!(right, 0.0, "right must be 0 when CH1 is left-only");
        assert!(left.abs() > 0.0, "left must carry CH1 audio");
    }

    #[test]
    fn test_stereo_mix_ch1_right_only() {
        // When CH1 is enabled on RIGHT only (NR51 bit 0), left channel must be 0.
        let mut apu = powered_apu();
        // NR50 = 0x77, NR51 = 0x01 (CH1 RIGHT only).
        apu.write16(0x0400_0080, 0x0177);
        apu.soundcnt_h = 0x0002;
        apu.write16(0x0400_0062, 0xF080);
        apu.write16(0x0400_0064, 0x8320);

        let (left, right) = apu.mix();
        assert_eq!(left, 0.0, "left must be 0 when CH1 is right-only");
        assert!(right.abs() > 0.0, "right must carry CH1 audio");
    }

    #[test]
    fn test_stereo_pcm_a_left_only() {
        // SOUNDCNT_H bit 9: DMA Sound A LEFT enable, bit 8 disabled → right must be 0.
        let mut apu = powered_apu();
        // bit 9 (A left) | bit 2 (A full vol) = 0x0204
        apu.soundcnt_h = 0x0204;
        apu.fifo_a.push(64);
        apu.fifo_a.advance();

        let (left, right) = apu.mix();
        assert!(left.abs() > 0.0, "left must carry PCM A audio");
        assert_eq!(right, 0.0, "right must be 0 when PCM A right is disabled");
    }

    #[test]
    fn test_stereo_pcm_a_right_only() {
        // SOUNDCNT_H bit 8: DMA Sound A RIGHT enable, bit 9 disabled → left must be 0.
        let mut apu = powered_apu();
        // bit 8 (A right) | bit 2 (A full vol) = 0x0104
        apu.soundcnt_h = 0x0104;
        apu.fifo_a.push(64);
        apu.fifo_a.advance();

        let (left, right) = apu.mix();
        assert_eq!(left, 0.0, "left must be 0 when PCM A left is disabled");
        assert!(right.abs() > 0.0, "right must carry PCM A audio");
    }

    #[test]
    fn test_stereo_pcm_b_left_only() {
        // SOUNDCNT_H bit 13: DMA Sound B LEFT enable, bit 12 disabled → right must be 0.
        let mut apu = powered_apu();
        // bit 13 (B left) | bit 3 (B full vol) = 0x2008
        apu.soundcnt_h = 0x2008;
        apu.fifo_b.push(64);
        apu.fifo_b.advance();

        let (left, right) = apu.mix();
        assert!(left.abs() > 0.0, "left must carry PCM B audio");
        assert_eq!(right, 0.0, "right must be 0 when PCM B right is disabled");
    }

    #[test]
    fn test_stereo_pcm_b_right_only() {
        // SOUNDCNT_H bit 12: DMA Sound B RIGHT enable, bit 13 disabled → left must be 0.
        let mut apu = powered_apu();
        // bit 12 (B right) | bit 3 (B full vol) = 0x1008
        apu.soundcnt_h = 0x1008;
        apu.fifo_b.push(64);
        apu.fifo_b.advance();

        let (left, right) = apu.mix();
        assert_eq!(left, 0.0, "left must be 0 when PCM B left is disabled");
        assert!(right.abs() > 0.0, "right must carry PCM B audio");
    }

    #[test]
    fn test_take_sample_returns_mono_downmix() {
        // take_sample() must still return Some(f32) as mono downmix for backward compat.
        let mut apu = powered_apu();
        apu.set_sample_rate(44_100.0);
        // PCM A at full vol, both L and R at equal level.
        apu.soundcnt_h = 0x0304; // A right | A left | A full vol
        apu.fifo_a.push(64);
        apu.fifo_a.advance();
        let cycles = (GBA_CLOCK_HZ / 44_100.0) as u32 + 1;
        apu.tick(cycles);
        let mono = apu.take_sample();
        assert!(mono.is_some(), "take_sample() must return Some(f32)");
        let s = mono.unwrap();
        assert!(
            (-1.0..=1.0).contains(&s),
            "mono sample must be in [-1.0, 1.0], got {s}"
        );
    }

    // ── CH3 dual-bank wave RAM tests ─────────────────────────────────────────

    #[test]
    fn test_ch3_sound3cnt_l_two_banks_bit() {
        let mut apu = powered_apu();
        // Bit 5 = 0x0020: wave RAM dimension = two banks
        apu.write16(0x0400_0070, 0x00A0); // bits 7 (DAC) + 5 (two_banks)
        assert!(apu.ch3.two_banks, "bit 5 should enable two-bank mode");
        // Clear bit 5
        apu.write16(0x0400_0070, 0x0080); // only bit 7 (DAC)
        assert!(
            !apu.ch3.two_banks,
            "bit 5 clear should disable two-bank mode"
        );
    }

    #[test]
    fn test_ch3_sound3cnt_l_bank_select_bit() {
        let mut apu = powered_apu();
        // Bit 6 = 0x0040: bank select = 1
        apu.write16(0x0400_0070, 0x00C0); // bits 7 (DAC) + 6 (bank_select=1)
        assert!(apu.ch3.bank_select, "bit 6 should select bank 1");
        // Clear bit 6
        apu.write16(0x0400_0070, 0x0080);
        assert!(!apu.ch3.bank_select, "bit 6 clear should select bank 0");
    }

    #[test]
    fn test_ch3_sound3cnt_l_bits_round_trip() {
        let mut apu = powered_apu();
        // Write bits 5+6+7
        apu.write16(0x0400_0070, 0x00E0); // two_banks + bank_select + dac_on
        let val = apu.read16(0x0400_0070);
        assert_eq!(val & 0x00E0, 0x00E0, "bits 5, 6, 7 should all round-trip");
        // Clear two_banks and bank_select
        apu.write16(0x0400_0070, 0x0080);
        let val = apu.read16(0x0400_0070);
        assert_eq!(val & 0x00E0, 0x0080, "only bit 7 should be set");
    }

    #[test]
    fn test_ch3_wave_ram_writes_to_other_bank() {
        let mut apu = powered_apu();
        // bank_select=0 (default): other bank = 1
        apu.write8(0x0400_0090, 0xAB);
        assert_eq!(
            apu.ch3.wave_ram[1][0], 0xAB,
            "write should go to bank 1 (other bank when bank_select=0)"
        );
        // bank_select=1: other bank = 0
        apu.write16(0x0400_0070, 0x0040); // bit 6 = bank_select=1 (DAC off)
        apu.write8(0x0400_0090, 0xCD);
        assert_eq!(
            apu.ch3.wave_ram[0][0], 0xCD,
            "write should go to bank 0 (other bank when bank_select=1)"
        );
    }

    #[test]
    fn test_ch3_wave_ram_reads_from_other_bank() {
        let mut apu = Apu::new();
        // Directly place different values in each bank
        apu.ch3.wave_ram[0][0] = 0x12;
        apu.ch3.wave_ram[1][0] = 0x34;
        // bank_select=0: reading wave RAM should return bank 1 content
        let val = apu.ch3.read_wave_ram(0);
        assert_eq!(val, 0x34, "read should return bank 1 when bank_select=0");
        // bank_select=1: reading wave RAM should return bank 0 content
        apu.ch3.bank_select = true;
        let val = apu.ch3.read_wave_ram(0);
        assert_eq!(val, 0x12, "read should return bank 0 when bank_select=1");
    }

    #[test]
    fn test_ch3_single_bank_playback_wraps_at_32() {
        let mut apu = powered_apu();
        // Write wave data to bank 1 (other bank when bank_select=0)
        for i in 0..16u8 {
            apu.write8(0x0400_0090 + i as u32, i);
        }
        // Switch bank_select to 1 so bank 1 plays
        apu.write16(0x0400_0070, 0x00C0); // bank_select=1 + DAC on
        apu.write16(0x0400_0072, 0x2000); // 100% output
        apu.write16(0x0400_0074, 0x8000); // trigger, freq=0
        assert!(apu.ch3.active);
        let period = 2048_u32 * 8;
        for _ in 0..31 {
            apu.ch3.tick(period);
        }
        assert_eq!(apu.ch3.wave_pos, 31, "wave_pos should reach 31");
        apu.ch3.tick(period); // one more step: should wrap back to 0
        assert_eq!(
            apu.ch3.wave_pos, 0,
            "single-bank mode should wrap to 0 after 32 samples"
        );
    }

    #[test]
    fn test_ch3_two_bank_plays_64_samples() {
        let mut apu = powered_apu();
        // bank_select=0 (default): write to bank 1 (other bank)
        for i in 0..16u8 {
            apu.write8(0x0400_0090 + i as u32, i * 0x11);
        }
        // Switch to two-bank mode, bank_select=1 so bank 1 plays first
        apu.write16(0x0400_0070, 0x00E0); // two_banks (bit5) + bank_select=1 (bit6) + DAC on (bit7)
        // Now write bank 0 data (other bank when bank_select=1)
        for i in 0..16u8 {
            apu.write8(0x0400_0090 + i as u32, 0xFF - i);
        }
        apu.write16(0x0400_0072, 0x2000); // 100% output
        apu.write16(0x0400_0074, 0x8000); // trigger, freq=0
        assert!(apu.ch3.active);
        let period = 2048_u32 * 8;
        for _ in 0..63 {
            apu.ch3.tick(period);
        }
        assert_eq!(
            apu.ch3.wave_pos, 63,
            "wave_pos should reach 63 in two-bank mode"
        );
        apu.ch3.tick(period); // wrap
        assert_eq!(
            apu.ch3.wave_pos, 0,
            "two-bank mode should wrap to 0 after 64 samples"
        );
    }

    #[test]
    fn test_ch3_two_bank_samples_cross_banks() {
        let mut apu = powered_apu();
        // bank_select=0: write distinctive data to bank 1 (other bank)
        // Bank 1: all bytes = 0xF0 (high nibble=0xF, low nibble=0x0)
        for i in 0..16u8 {
            apu.write8(0x0400_0090 + i as u32, 0xF0);
        }
        // Switch to two-bank + bank_select=1 so bank 1 plays first
        apu.write16(0x0400_0070, 0x00E0); // two_banks + bank_select=1 + DAC on
        // Now bank 0 is the other bank: write 0x0A (high nibble=0x0, low nibble=0xA)
        for i in 0..16u8 {
            apu.write8(0x0400_0090 + i as u32, 0x0A);
        }
        apu.write16(0x0400_0072, 0x2000); // 100% output
        apu.write16(0x0400_0074, 0x8000); // trigger, freq=0
        assert!(apu.ch3.active);
        let period = 2048_u32 * 8;
        // Samples 0-31 come from bank 1 (0xF0 bytes → nibbles 0xF, 0x0, 0xF, 0x0, ...)
        assert_eq!(apu.ch3.current_sample, 0xF, "first sample from bank 1");
        apu.ch3.tick(period);
        assert_eq!(apu.ch3.current_sample, 0x0, "second sample from bank 1");
        // Advance to sample 32 (first sample from bank 0)
        for _ in 0..30 {
            apu.ch3.tick(period);
        }
        assert_eq!(apu.ch3.wave_pos, 31);
        apu.ch3.tick(period); // advance to wave_pos=32, now in bank 0
        assert_eq!(apu.ch3.wave_pos, 32);
        // Bank 0 has 0x0A bytes → high nibble = 0x0
        assert_eq!(
            apu.ch3.current_sample, 0x0,
            "first sample from bank 0 (high nibble of 0x0A)"
        );
        apu.ch3.tick(period); // wave_pos=33, low nibble of 0x0A = 0xA
        assert_eq!(
            apu.ch3.current_sample, 0xA,
            "second sample from bank 0 (low nibble of 0x0A)"
        );
    }

    #[test]
    fn test_ch3_power_off_preserves_both_banks() {
        let mut apu = powered_apu();
        // Fill both banks with distinctive data
        apu.ch3.wave_ram[0] = [0xAA; 16];
        apu.ch3.wave_ram[1] = [0xBB; 16];
        // Power off
        apu.write16(0x0400_0084, 0x0000);
        // Both wave RAM banks should survive
        assert_eq!(
            apu.ch3.wave_ram[0], [0xAA; 16],
            "bank 0 should survive power-off"
        );
        assert_eq!(
            apu.ch3.wave_ram[1], [0xBB; 16],
            "bank 1 should survive power-off"
        );
    }
}
